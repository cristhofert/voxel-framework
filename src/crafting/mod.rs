//! Crafting system — shapeless recipes, registry, and in-game crafting menu.
//!
//! # Data model
//!
//! [`CraftingRegistry<I>`] is a Bevy resource that holds a list of
//! [`CraftingRecipe<I>`] values. Each recipe describes a set of input
//! ingredients and output items. Recipes are *shapeless* — only item types and
//! counts matter, not arrangement in a grid.
//!
//! # Crafting logic
//!
//! [`try_craft`] is a pure function: it verifies that the inventory holds
//! enough of every ingredient and produces the outputs. Built-in containers
//! exchange atomically; legacy external containers may partially mutate on
//! failure. [`can_craft`] checks ingredients only, without side effects.
//!
//! # UI
//!
//! [`CraftingPlugin<I>`] adds a crafting menu toggled by the `C` key.
//! The menu lists every registered recipe and highlights those the player can
//! currently craft. Clicking a highlighted row invokes [`try_craft`].
//!
//! # Integration
//!
//! ```no_run
//! use bevy::prelude::*;
//! use voxel_framework::crafting::{CraftingPlugin, CraftingRecipe, CraftingIngredient, CraftingRegistry};
//! use voxel_framework::items::DefaultItem;
//!
//! App::new()
//!     .add_plugins(CraftingPlugin::<DefaultItem>::new())
//!     .add_systems(Startup, register_recipes);
//!
//! fn register_recipes(mut registry: ResMut<CraftingRegistry<DefaultItem>>) {
//!     registry.add(CraftingRecipe {
//!         name: "Crystal from Ore",
//!         ingredients: vec![CraftingIngredient { item: DefaultItem::IronOre, count: 4 }],
//!         outputs:     vec![CraftingIngredient { item: DefaultItem::Crystal,  count: 1 }],
//!     });
//! }
//! ```

use std::marker::PhantomData;

use bevy::prelude::*;

use crate::items::ItemType;
use crate::items::inventory::{
    Container, ContainerComponent, ContainerExchangeError, PrimaryContainer, normalize_quantities,
};

// ─── Data layer ───────────────────────────────────────────────────────────────

/// A single ingredient entry: the item type and the required or produced quantity.
#[derive(Debug, Clone)]
pub struct CraftingIngredient<I> {
    /// The item type consumed or produced.
    pub item: I,
    /// How many items are consumed or produced.
    pub count: u32,
}

/// A shapeless crafting recipe: consume [`ingredients`](CraftingRecipe::ingredients),
/// produce [`outputs`](CraftingRecipe::outputs).
///
/// "Shapeless" means arrangement in a crafting grid is not modelled — only item
/// types and counts matter. Duplicate entries aggregate by item equality in
/// first-occurrence order with checked `u32` totals. Zero entries are ignored;
/// empty recipes are allowed. Positive default/empty sentinels are invalid.
#[derive(Debug, Clone)]
pub struct CraftingRecipe<I> {
    /// Short human-readable name shown in the crafting menu.
    pub name: &'static str,
    /// Items consumed from the player's inventory when this recipe is crafted.
    pub ingredients: Vec<CraftingIngredient<I>>,
    /// Items added to the player's inventory when this recipe is crafted.
    pub outputs: Vec<CraftingIngredient<I>>,
}

/// Registry of all crafting recipes available in the game.
///
/// Inserted automatically by [`CraftingPlugin`] as an empty resource.
/// Populate it from a startup system before the crafting menu is first opened.
#[derive(Resource, Debug, Clone, Default)]
pub struct CraftingRegistry<I: ItemType> {
    /// Ordered list of registered recipes. Indices are stable for the session.
    pub recipes: Vec<CraftingRecipe<I>>,
}

impl<I: ItemType> CraftingRegistry<I> {
    /// Appends a recipe to the registry.
    pub fn add(&mut self, recipe: CraftingRecipe<I>) {
        self.recipes.push(recipe);
    }
}

// ─── Pure crafting functions ──────────────────────────────────────────────────

/// Reason a craft attempt failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CraftingError {
    /// The index is missing, or the recipe contains a sentinel or quantity overflow.
    InvalidRecipe,
    /// The inventory does not hold enough of one or more ingredients.
    InsufficientIngredients,
    /// The container is invalid or lacks slot/numeric capacity for the outputs.
    InventoryFull,
}

// Both lists are validated before ingredients or capacity are considered.
fn normalized_recipe<I: ItemType>(
    recipe: &CraftingRecipe<I>,
) -> Result<NormalizedRecipe<I>, CraftingError> {
    let normalize = |entries: &[CraftingIngredient<I>]| {
        let quantities: Vec<_> = entries
            .iter()
            .map(|entry| (entry.item, entry.count))
            .collect();
        normalize_quantities(&quantities).map_err(|_| CraftingError::InvalidRecipe)
    };
    Ok(NormalizedRecipe {
        remove: normalize(&recipe.ingredients)?,
        add: normalize(&recipe.outputs)?,
    })
}

struct NormalizedRecipe<I> {
    remove: Vec<(I, u32)>,
    add: Vec<(I, u32)>,
}

/// Returns `true` for a valid recipe whose aggregate ingredients are available.
///
/// Does **not** mutate or check output capacity. Invalid outputs still make the
/// recipe invalid, even though this check otherwise concerns ingredients only.
pub fn can_craft<I: ItemType>(
    registry: &CraftingRegistry<I>,
    inventory: &impl Container<I>,
    recipe_index: usize,
) -> bool {
    registry
        .recipes
        .get(recipe_index)
        .and_then(|recipe| normalized_recipe(recipe).ok())
        .is_some_and(|recipe| {
            recipe
                .remove
                .iter()
                .all(|&(item, count)| inventory.count_of(item) >= count)
        })
}

/// Attempts to craft a normalized recipe, consuming ingredients and producing outputs.
///
/// # Atomicity
///
/// Built-in containers and external containers opting into
/// [`Container::try_exchange_atomic`] complete the whole exchange or leave the
/// container unchanged, evaluating capacity after consumption.
///
/// Legacy containers returning `None` use conservative **current** free space
/// (no credit for slots consumption might free). All preflight checks happen
/// before mutation, but a subsequent short removal or leftover output returns
/// an error immediately and **may leave partial mutation**. No rollback is
/// invented for arbitrary external storage. Success requires exact removal and
/// insertion returns for every normalized quantity.
pub fn try_craft<I: ItemType>(
    registry: &CraftingRegistry<I>,
    inventory: &mut impl Container<I>,
    recipe_index: usize,
) -> Result<(), CraftingError> {
    let recipe = normalized_recipe(
        registry
            .recipes
            .get(recipe_index)
            .ok_or(CraftingError::InvalidRecipe)?,
    )?;

    if let Some(result) = inventory.try_exchange_atomic(&recipe.remove, &recipe.add) {
        // The public neutral enum is extensible; future errors remain failures.
        #[allow(unreachable_patterns)]
        return result.map_err(|error| match error {
            ContainerExchangeError::InvalidRequest => CraftingError::InvalidRecipe,
            ContainerExchangeError::InsufficientItems => CraftingError::InsufficientIngredients,
            ContainerExchangeError::InvalidContainer | ContainerExchangeError::CapacityExceeded => {
                CraftingError::InventoryFull
            }
            _ => CraftingError::InventoryFull,
        });
    }

    for &(item, count) in &recipe.remove {
        if inventory.count_of(item) < count {
            return Err(CraftingError::InsufficientIngredients);
        }
    }
    let occupied = inventory.iter_slots().filter(|slot| slot.is_some()).count();
    let free_slots = inventory
        .capacity()
        .checked_sub(occupied)
        .ok_or(CraftingError::InventoryFull)?;
    let new_types = recipe
        .add
        .iter()
        .filter(|&&(item, _)| inventory.count_of(item) == 0)
        .count();
    if new_types > free_slots {
        return Err(CraftingError::InventoryFull);
    }
    for &(item, count) in &recipe.add {
        let removed = recipe
            .remove
            .iter()
            .find(|&&(input, _)| input == item)
            .map_or(0, |&(_, count)| count);
        let projected = u64::from(inventory.count_of(item))
            .checked_sub(u64::from(removed))
            .ok_or(CraftingError::InsufficientIngredients)?
            + u64::from(count);
        if projected > u64::from(u32::MAX) {
            return Err(CraftingError::InventoryFull);
        }
    }
    for (item, count) in recipe.remove {
        if inventory.remove(item, count) != count {
            return Err(CraftingError::InsufficientIngredients);
        }
    }
    for (item, count) in recipe.add {
        if inventory.add(item, count) != 0 {
            return Err(CraftingError::InventoryFull);
        }
    }
    Ok(())
}

// ─── UI state ────────────────────────────────────────────────────────────────

/// Runtime state of the crafting menu.
#[derive(Resource, Default)]
pub struct CraftingMenuState {
    /// Whether the crafting menu is currently visible.
    pub visible: bool,
    /// Dirty flag: set to `true` to trigger a full UI rebuild on next frame.
    rebuild: bool,
}

// ─── UI markers ───────────────────────────────────────────────────────────────

/// Marker component on the root crafting menu panel.
#[derive(Component)]
struct CraftingMenuRoot;

/// Marker component on each recipe row in the crafting menu.
///
/// `usize::MAX` is used as a sentinel for header/footer rows that are not
/// interactive recipes.
#[derive(Component)]
struct CraftingRecipeRow {
    recipe_index: usize,
}

// ─── Systems ──────────────────────────────────────────────────────────────────

fn spawn_crafting_menu(mut commands: Commands) {
    commands.spawn((
        CraftingMenuRoot,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(10.0),
            top: Val::Px(60.0),
            width: Val::Px(340.0),
            max_height: Val::Vh(75.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(10.0)),
            row_gap: Val::Px(4.0),
            overflow: Overflow::clip_y(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.04, 0.04, 0.88)),
        Visibility::Hidden,
    ));
}

fn toggle_crafting_menu(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<CraftingMenuState>) {
    if keys.just_pressed(KeyCode::KeyC) {
        state.visible = !state.visible;
        if state.visible {
            state.rebuild = true;
        }
    }
    // ESC closes crafting menu if open (without consuming the event — pause menu
    // handles its own ESC separately).
    if keys.just_pressed(KeyCode::Escape) && state.visible {
        state.visible = false;
    }
}

fn sync_crafting_visibility(
    state: Res<CraftingMenuState>,
    mut root_q: Query<&mut Visibility, With<CraftingMenuRoot>>,
) {
    if !state.is_changed() {
        return;
    }
    let Ok(mut vis) = root_q.single_mut() else {
        return;
    };
    *vis = if state.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

fn rebuild_crafting_menu<I: ItemType>(
    mut commands: Commands,
    mut state: ResMut<CraftingMenuState>,
    registry: Res<CraftingRegistry<I>>,
    root_q: Query<Entity, With<CraftingMenuRoot>>,
    row_q: Query<Entity, With<CraftingRecipeRow>>,
) {
    if !state.rebuild {
        return;
    }
    state.rebuild = false;

    // Despawn stale rows.
    for e in row_q.iter() {
        commands.entity(e).despawn();
    }

    let Ok(root) = root_q.single() else {
        return;
    };

    // Header.
    let header = commands
        .spawn((
            CraftingRecipeRow {
                recipe_index: usize::MAX,
            },
            Node {
                padding: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
            Text::new("-- Crafting (C to close) --"),
            TextFont {
                font_size: 15.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(root).add_child(header);

    if registry.recipes.is_empty() {
        let empty = commands
            .spawn((
                CraftingRecipeRow {
                    recipe_index: usize::MAX,
                },
                Text::new("No recipes registered."),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.55, 0.55)),
            ))
            .id();
        commands.entity(root).add_child(empty);
        return;
    }

    // One row per recipe.
    for (i, recipe) in registry.recipes.iter().enumerate() {
        let ing_str = recipe
            .ingredients
            .iter()
            .map(|ing| format!("{} ×{}", ing.item.name(), ing.count))
            .collect::<Vec<_>>()
            .join(", ");
        let out_str = recipe
            .outputs
            .iter()
            .map(|out| format!("{} ×{}", out.item.name(), out.count))
            .collect::<Vec<_>>()
            .join(", ");

        let label = format!("[{}]  {} > {}", recipe.name, ing_str, out_str);

        let row = commands
            .spawn((
                CraftingRecipeRow { recipe_index: i },
                Button,
                Node {
                    padding: UiRect::all(Val::Px(6.0)),
                    width: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.90)),
                Text::new(label),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.70, 0.70, 0.70)),
            ))
            .id();
        commands.entity(root).add_child(row);
    }

    // Footer hint.
    let footer = commands
        .spawn((
            CraftingRecipeRow {
                recipe_index: usize::MAX,
            },
            Node {
                padding: UiRect::top(Val::Px(6.0)),
                ..default()
            },
            Text::new("Green = craftable  |  click to craft"),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(0.45, 0.45, 0.45)),
        ))
        .id();
    commands.entity(root).add_child(footer);
}

fn update_crafting_row_colors<I: ItemType>(
    mut row_q: Query<(
        &CraftingRecipeRow,
        &mut BackgroundColor,
        &mut TextColor,
        &Interaction,
    )>,
    registry: Res<CraftingRegistry<I>>,
    container_query: Query<&ContainerComponent<I>, With<PrimaryContainer>>,
    state: Res<CraftingMenuState>,
) {
    if !state.visible {
        return;
    }
    let Ok(container) = container_query.single() else {
        return;
    };
    for (row, mut bg, mut tc, interaction) in row_q.iter_mut() {
        if row.recipe_index == usize::MAX {
            continue;
        }
        let craftable = can_craft(&registry, &container.0, row.recipe_index);
        let hovered = matches!(interaction, Interaction::Hovered | Interaction::Pressed);

        *bg = match (craftable, hovered) {
            (true, true) => BackgroundColor(Color::srgba(0.18, 0.50, 0.18, 0.95)),
            (true, false) => BackgroundColor(Color::srgba(0.10, 0.28, 0.10, 0.90)),
            (false, true) => BackgroundColor(Color::srgba(0.25, 0.25, 0.25, 0.90)),
            (false, false) => BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.90)),
        };
        *tc = if craftable {
            TextColor(Color::srgb(0.82, 1.0, 0.82))
        } else {
            TextColor(Color::srgb(0.45, 0.45, 0.45))
        };
    }
}

fn handle_craft_click<I: ItemType>(
    row_q: Query<(&CraftingRecipeRow, &Interaction), Changed<Interaction>>,
    registry: Res<CraftingRegistry<I>>,
    mut container_query: Query<&mut ContainerComponent<I>, With<PrimaryContainer>>,
    state: Res<CraftingMenuState>,
) {
    if !state.visible {
        return;
    }
    let Ok(mut container) = container_query.single_mut() else {
        return;
    };
    for (row, interaction) in row_q.iter() {
        if row.recipe_index == usize::MAX {
            continue;
        }
        if matches!(interaction, Interaction::Pressed) {
            let _ = try_craft(&registry, &mut container.0, row.recipe_index);
        }
    }
}

// ─── Plugins ──────────────────────────────────────────────────────────────────

/// Core crafting plugin — registers [`CraftingRegistry<I>`] only.
///
/// No UI, no input, no systems. Use alongside [`CraftingUiPlugin<I>`] when you
/// want the built-in `C`-key crafting menu, or provide your own UI by reading
/// [`CraftingRegistry<I>`] and calling [`can_craft`] / [`try_craft`] directly.
///
/// Mirrors the [`crate::world::MiningPlugin`] / [`crate::world::MiningInputPlugin`] split pattern.
pub struct CraftingPlugin<I: ItemType> {
    _m: PhantomData<I>,
}

impl<I: ItemType> CraftingPlugin<I> {
    pub fn new() -> Self {
        Self { _m: PhantomData }
    }
}

impl<I: ItemType> Default for CraftingPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: ItemType> Plugin for CraftingPlugin<I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<CraftingRegistry<I>>();
    }
}

/// Batteries-included crafting UI plugin.
///
/// Adds [`CraftingMenuState`], spawns the in-game crafting panel, and wires the
/// `C`-key toggle, recipe highlighting, and click-to-craft. Requires
/// [`CraftingPlugin<I>`] to have been added first (for the registry).
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::crafting::{CraftingPlugin, CraftingUiPlugin, CraftingRecipe, CraftingIngredient};
/// use voxel_framework::items::DefaultItem;
///
/// App::new()
///     .add_plugins(CraftingPlugin::<DefaultItem>::new())
///     .add_plugins(CraftingUiPlugin::<DefaultItem>::new())
///     .add_systems(Startup, register_recipes);
///
/// fn register_recipes(mut registry: ResMut<voxel_framework::crafting::CraftingRegistry<DefaultItem>>) {
///     registry.add(CraftingRecipe {
///         name: "Iron Block",
///         ingredients: vec![CraftingIngredient { item: DefaultItem::IronOre, count: 9 }],
///         outputs:     vec![CraftingIngredient { item: DefaultItem::Crystal,  count: 1 }],
///     });
/// }
/// ```
pub struct CraftingUiPlugin<I: ItemType> {
    _m: PhantomData<I>,
}

impl<I: ItemType> CraftingUiPlugin<I> {
    pub fn new() -> Self {
        Self { _m: PhantomData }
    }
}

impl<I: ItemType> Default for CraftingUiPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: ItemType> Plugin for CraftingUiPlugin<I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<CraftingMenuState>();
        app.add_systems(Startup, spawn_crafting_menu);
        app.add_systems(
            Update,
            (
                toggle_crafting_menu,
                sync_crafting_visibility,
                rebuild_crafting_menu::<I>,
                update_crafting_row_colors::<I>,
                handle_craft_click::<I>,
            )
                .chain(),
        );
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{DefaultItem, Inventory};

    fn make_registry() -> CraftingRegistry<DefaultItem> {
        let mut r = CraftingRegistry::default();
        r.add(CraftingRecipe {
            name: "Crystal from Ore",
            ingredients: vec![CraftingIngredient {
                item: DefaultItem::IronOre,
                count: 4,
            }],
            outputs: vec![CraftingIngredient {
                item: DefaultItem::Crystal,
                count: 1,
            }],
        });
        r.add(CraftingRecipe {
            name: "Rare from Ore+Crystal",
            ingredients: vec![
                CraftingIngredient {
                    item: DefaultItem::IronOre,
                    count: 2,
                },
                CraftingIngredient {
                    item: DefaultItem::Crystal,
                    count: 1,
                },
            ],
            outputs: vec![CraftingIngredient {
                item: DefaultItem::RareMineral,
                count: 1,
            }],
        });
        r
    }

    #[test]
    fn can_craft_true_when_enough_items() {
        let registry = make_registry();
        let mut inv = Inventory::new(9);
        inv.add(DefaultItem::IronOre, 4);
        assert!(can_craft(&registry, &inv, 0));
    }

    #[test]
    fn can_craft_false_when_insufficient() {
        let registry = make_registry();
        let mut inv = Inventory::new(9);
        inv.add(DefaultItem::IronOre, 3); // need 4
        assert!(!can_craft(&registry, &inv, 0));
    }

    #[test]
    fn can_craft_false_for_invalid_index() {
        let registry = make_registry();
        let inv = Inventory::<DefaultItem>::new(9);
        assert!(!can_craft(&registry, &inv, 99));
    }

    #[test]
    fn try_craft_consumes_ingredients_and_produces_outputs() {
        let registry = make_registry();
        let mut inv = Inventory::new(9);
        inv.add(DefaultItem::IronOre, 5);

        let result = try_craft(&registry, &mut inv, 0);
        assert_eq!(result, Ok(()));
        assert_eq!(inv.count_of(DefaultItem::IronOre), 1); // 5 - 4
        assert_eq!(inv.count_of(DefaultItem::Crystal), 1);
    }

    #[test]
    fn try_craft_fails_with_insufficient_ingredients() {
        let registry = make_registry();
        let mut inv = Inventory::new(9);
        inv.add(DefaultItem::IronOre, 2);

        let result = try_craft(&registry, &mut inv, 0);
        assert_eq!(result, Err(CraftingError::InsufficientIngredients));
        // Inventory unchanged.
        assert_eq!(inv.count_of(DefaultItem::IronOre), 2);
    }

    #[test]
    fn try_craft_fails_with_invalid_recipe() {
        let registry = make_registry();
        let mut inv = Inventory::new(9);
        assert_eq!(
            try_craft(&registry, &mut inv, 99),
            Err(CraftingError::InvalidRecipe)
        );
    }

    #[test]
    fn try_craft_multi_ingredient_recipe() {
        let registry = make_registry();
        let mut inv = Inventory::new(9);
        inv.add(DefaultItem::IronOre, 2);
        inv.add(DefaultItem::Crystal, 1);

        let result = try_craft(&registry, &mut inv, 1);
        assert_eq!(result, Ok(()));
        assert_eq!(inv.count_of(DefaultItem::IronOre), 0);
        assert_eq!(inv.count_of(DefaultItem::Crystal), 0);
        assert_eq!(inv.count_of(DefaultItem::RareMineral), 1);
    }

    #[test]
    fn try_craft_inventory_full_returns_error() {
        let registry = make_registry();
        // Retaining an ingredient stack leaves no slot for the new output.
        let mut inv = Inventory::new(1);
        inv.add(DefaultItem::IronOre, 5); // one remains after consumption

        // Now inventory is full (1 slot used).
        // Crystal is a new type — needs a free slot, but none available.
        let result = try_craft(&registry, &mut inv, 0);
        assert_eq!(result, Err(CraftingError::InventoryFull));
        // Ingredients not consumed.
        assert_eq!(inv.count_of(DefaultItem::IronOre), 5);
    }

    fn registry_for<I: ItemType>(remove: &[(I, u32)], add: &[(I, u32)]) -> CraftingRegistry<I> {
        let mut registry = CraftingRegistry::default();
        let entries = |quantities: &[(I, u32)]| {
            quantities
                .iter()
                .map(|&(item, count)| CraftingIngredient { item, count })
                .collect()
        };
        registry.add(CraftingRecipe {
            name: "regression",
            ingredients: entries(remove),
            outputs: entries(add),
        });
        registry
    }

    #[test]
    fn duplicate_ingredients_require_aggregate_and_failure_preserves_layout() {
        let registry = registry_for(
            &[(DefaultItem::IronOre, 2), (DefaultItem::IronOre, 2)],
            &[(DefaultItem::Crystal, 1)],
        );
        let mut inv = Inventory::new(2);
        inv.add(DefaultItem::IronOre, 3);
        let before = inv.slots.clone();
        assert!(!can_craft(&registry, &inv, 0));
        assert_eq!(
            try_craft(&registry, &mut inv, 0),
            Err(CraftingError::InsufficientIngredients)
        );
        assert_eq!(inv.slots, before);
        inv.add(DefaultItem::IronOre, 1);
        assert!(can_craft(&registry, &inv, 0));
        assert_eq!(try_craft(&registry, &mut inv, 0), Ok(()));
        assert_eq!(inv.count_of(DefaultItem::IronOre), 0);
        assert_eq!(inv.count_of(DefaultItem::Crystal), 1);
    }

    #[test]
    fn full_builtin_consumption_frees_slot_for_duplicate_outputs() {
        let registry = registry_for(
            &[(DefaultItem::IronOre, 4)],
            &[(DefaultItem::Crystal, 1), (DefaultItem::Crystal, 2)],
        );
        let mut inv = Inventory::new(1);
        inv.add(DefaultItem::IronOre, 4);
        assert!(can_craft(&registry, &inv, 0));
        assert_eq!(try_craft(&registry, &mut inv, 0), Ok(()));
        assert_eq!(inv.count_of(DefaultItem::Crystal), 3);
        assert_eq!(inv.capacity, 1);
    }

    #[test]
    fn existing_output_stacks_and_same_input_use_post_consumption_headroom() {
        let registry = registry_for(
            &[(DefaultItem::IronOre, 4)],
            &[
                (DefaultItem::IronOre, 2),
                (DefaultItem::IronOre, 2),
                (DefaultItem::Crystal, 3),
            ],
        );
        let mut inv = Inventory::new(2);
        inv.add(DefaultItem::IronOre, u32::MAX);
        inv.add(DefaultItem::Crystal, 1);
        assert_eq!(try_craft(&registry, &mut inv, 0), Ok(()));
        assert_eq!(inv.count_of(DefaultItem::IronOre), u32::MAX);
        assert_eq!(inv.count_of(DefaultItem::Crystal), 4);
    }

    #[test]
    fn projected_overflow_or_late_capacity_failure_is_atomic_builtin() {
        for outputs in [
            vec![(DefaultItem::Crystal, 1)],
            vec![(DefaultItem::Wood, 1), (DefaultItem::Sand, 1)],
        ] {
            let registry = registry_for(&[(DefaultItem::IronOre, 1)], &outputs);
            let mut inv = Inventory::new(3);
            inv.add(DefaultItem::IronOre, 2);
            inv.add(DefaultItem::Crystal, u32::MAX);
            let before = inv.slots.clone();
            assert!(can_craft(&registry, &inv, 0));
            assert_eq!(
                try_craft(&registry, &mut inv, 0),
                Err(CraftingError::InventoryFull)
            );
            assert_eq!(inv.slots, before);
        }
    }

    #[test]
    fn recipe_overflow_and_positive_sentinels_are_invalid_before_mutation() {
        let invalid_lists = [
            vec![(DefaultItem::Empty, 1)],
            vec![(DefaultItem::IronOre, u32::MAX), (DefaultItem::IronOre, 1)],
        ];
        for invalid in invalid_lists {
            for registry in [registry_for(&invalid, &[]), registry_for(&[], &invalid)] {
                let mut inv = Inventory::new(1);
                inv.add(DefaultItem::IronOre, u32::MAX);
                let before = inv.slots.clone();
                assert!(!can_craft(&registry, &inv, 0));
                assert_eq!(
                    try_craft(&registry, &mut inv, 0),
                    Err(CraftingError::InvalidRecipe)
                );
                assert_eq!(inv.slots, before);
            }
        }
    }

    #[test]
    fn zero_and_empty_recipe_semantics_and_ingredients_only_check() {
        let mut inv = Inventory::<DefaultItem>::new(0);
        for registry in [
            registry_for(&[], &[]),
            registry_for(&[(DefaultItem::Empty, 0)], &[(DefaultItem::Empty, 0)]),
        ] {
            assert!(can_craft(&registry, &inv, 0));
            assert_eq!(try_craft(&registry, &mut inv, 0), Ok(()));
        }
        let registry = registry_for(&[], &[(DefaultItem::Crystal, 1)]);
        assert!(can_craft(&registry, &inv, 0));
        assert_eq!(
            try_craft(&registry, &mut inv, 0),
            Err(CraftingError::InventoryFull)
        );
    }

    #[test]
    fn malformed_inventory_maps_to_inventory_full_without_mutation() {
        let registry = registry_for(&[], &[]);
        let mut inv = Inventory::<DefaultItem>::new(1);
        inv.capacity = 2;
        let before = inv.slots.clone();
        assert_eq!(
            try_craft(&registry, &mut inv, 0),
            Err(CraftingError::InventoryFull)
        );
        assert_eq!(inv.slots, before);
        assert_eq!(inv.capacity, 2);
    }

    // A legacy implementor with no Clone/Debug/Serialize or exchange method.
    struct LegacyContainer {
        inventory: Inventory<DefaultItem>,
        remove_limit: Option<u32>,
        add_limit: Option<u32>,
        removes: Vec<(DefaultItem, u32)>,
        adds: Vec<(DefaultItem, u32)>,
        reported_capacity: Option<usize>,
    }

    impl LegacyContainer {
        fn new(capacity: usize) -> Self {
            Self {
                inventory: Inventory::new(capacity),
                remove_limit: None,
                add_limit: None,
                removes: vec![],
                adds: vec![],
                reported_capacity: None,
            }
        }
    }

    impl Container<DefaultItem> for LegacyContainer {
        fn add(&mut self, item: DefaultItem, count: u32) -> u32 {
            self.adds.push((item, count));
            let accepted = count.min(self.add_limit.unwrap_or(count));
            count - accepted + self.inventory.add(item, accepted)
        }
        fn remove(&mut self, item: DefaultItem, count: u32) -> u32 {
            self.removes.push((item, count));
            self.inventory
                .remove(item, count.min(self.remove_limit.unwrap_or(count)))
        }
        fn count_of(&self, item: DefaultItem) -> u32 {
            self.inventory.count_of(item)
        }
        fn capacity(&self) -> usize {
            self.reported_capacity.unwrap_or(self.inventory.capacity)
        }
        fn is_full(&self) -> bool {
            self.inventory.is_full()
        }
        fn iter_slots(
            &self,
        ) -> impl Iterator<Item = Option<&crate::items::InventorySlot<DefaultItem>>> {
            self.inventory.slots.iter().map(Option::as_ref)
        }
        fn slot_at(&self, index: usize) -> Option<&crate::items::InventorySlot<DefaultItem>> {
            self.inventory.slots.get(index).and_then(Option::as_ref)
        }
    }

    #[test]
    fn legacy_default_hook_is_nonmutating_and_normalized_fallback_succeeds() {
        let mut legacy = LegacyContainer::new(2);
        legacy.inventory.add(DefaultItem::IronOre, 4);
        let before = legacy.inventory.slots.clone();
        assert_eq!(
            legacy.try_exchange_atomic(&[(DefaultItem::IronOre, 4)], &[]),
            None
        );
        assert_eq!(legacy.inventory.slots, before);
        let registry = registry_for(
            &[(DefaultItem::IronOre, 2), (DefaultItem::IronOre, 2)],
            &[(DefaultItem::Crystal, 1), (DefaultItem::Crystal, 2)],
        );
        assert_eq!(try_craft(&registry, &mut legacy, 0), Ok(()));
        assert_eq!(legacy.removes, vec![(DefaultItem::IronOre, 4)]);
        assert_eq!(legacy.adds, vec![(DefaultItem::Crystal, 3)]);
        assert_eq!(legacy.count_of(DefaultItem::Crystal), 3);
    }

    #[test]
    fn legacy_preflight_is_conservative_and_ingredients_remain_aggregate() {
        let registry = registry_for(
            &[(DefaultItem::IronOre, 2), (DefaultItem::IronOre, 2)],
            &[(DefaultItem::Crystal, 1)],
        );
        let mut legacy = LegacyContainer::new(1);
        legacy.inventory.add(DefaultItem::IronOre, 3);
        let before = legacy.inventory.slots.clone();
        assert_eq!(
            try_craft(&registry, &mut legacy, 0),
            Err(CraftingError::InsufficientIngredients)
        );
        assert_eq!(legacy.inventory.slots, before);
        legacy.inventory.add(DefaultItem::IronOre, 1);
        let before = legacy.inventory.slots.clone();
        assert!(can_craft(&registry, &legacy, 0));
        assert_eq!(
            try_craft(&registry, &mut legacy, 0),
            Err(CraftingError::InventoryFull)
        );
        assert_eq!(legacy.inventory.slots, before);
        assert!(legacy.removes.is_empty());
        assert!(legacy.adds.is_empty());
    }

    #[test]
    fn legacy_short_removal_stops_before_later_inputs_or_any_outputs() {
        let registry = registry_for(
            &[(DefaultItem::IronOre, 4), (DefaultItem::Wood, 1)],
            &[(DefaultItem::Crystal, 1)],
        );
        let mut legacy = LegacyContainer::new(3);
        legacy.inventory.add(DefaultItem::IronOre, 4);
        legacy.inventory.add(DefaultItem::Wood, 1);
        legacy.remove_limit = Some(2);
        assert_eq!(
            try_craft(&registry, &mut legacy, 0),
            Err(CraftingError::InsufficientIngredients)
        );
        assert_eq!(legacy.removes, vec![(DefaultItem::IronOre, 4)]);
        assert!(legacy.adds.is_empty());
        assert_eq!(legacy.count_of(DefaultItem::IronOre), 2); // No invented rollback.
        assert_eq!(legacy.count_of(DefaultItem::Wood), 1);
    }

    #[test]
    fn legacy_partial_add_stops_before_later_outputs_without_false_success() {
        let registry = registry_for(
            &[(DefaultItem::IronOre, 4)],
            &[(DefaultItem::Crystal, 3), (DefaultItem::Wood, 1)],
        );
        let mut legacy = LegacyContainer::new(3);
        legacy.inventory.add(DefaultItem::IronOre, 4);
        legacy.add_limit = Some(1);
        assert_eq!(
            try_craft(&registry, &mut legacy, 0),
            Err(CraftingError::InventoryFull)
        );
        assert_eq!(legacy.removes, vec![(DefaultItem::IronOre, 4)]);
        assert_eq!(legacy.adds, vec![(DefaultItem::Crystal, 3)]);
        assert_eq!(legacy.count_of(DefaultItem::IronOre), 0);
        assert_eq!(legacy.count_of(DefaultItem::Crystal), 1);
        assert_eq!(legacy.count_of(DefaultItem::Wood), 0);
    }

    #[test]
    fn legacy_numeric_and_capacity_preflight_fail_without_mutation() {
        let registry = registry_for(&[], &[(DefaultItem::Crystal, 1)]);
        let mut legacy = LegacyContainer::new(1);
        legacy.inventory.add(DefaultItem::Crystal, u32::MAX);
        let before = legacy.inventory.slots.clone();
        assert_eq!(
            try_craft(&registry, &mut legacy, 0),
            Err(CraftingError::InventoryFull)
        );
        assert_eq!(legacy.inventory.slots, before);
        legacy.reported_capacity = Some(0); // checked subtraction, no panic.
        let empty = registry_for(&[], &[]);
        assert_eq!(
            try_craft(&empty, &mut legacy, 0),
            Err(CraftingError::InventoryFull)
        );
        assert!(legacy.removes.is_empty());
        assert!(legacy.adds.is_empty());
    }

    #[test]
    fn legacy_same_input_outputs_and_duplicate_existing_outputs() {
        let registry = registry_for(
            &[(DefaultItem::IronOre, 4)],
            &[
                (DefaultItem::IronOre, 1),
                (DefaultItem::Crystal, 2),
                (DefaultItem::IronOre, 3),
                (DefaultItem::Crystal, 1),
            ],
        );
        let mut legacy = LegacyContainer::new(2);
        legacy.inventory.add(DefaultItem::IronOre, u32::MAX);
        legacy.inventory.add(DefaultItem::Crystal, 1);
        assert_eq!(try_craft(&registry, &mut legacy, 0), Ok(()));
        assert_eq!(legacy.removes, vec![(DefaultItem::IronOre, 4)]);
        assert_eq!(
            legacy.adds,
            vec![(DefaultItem::IronOre, 4), (DefaultItem::Crystal, 3)]
        );
        assert_eq!(legacy.count_of(DefaultItem::IronOre), u32::MAX);
        assert_eq!(legacy.count_of(DefaultItem::Crystal), 4);
    }

    struct OptedInContainer {
        legacy: LegacyContainer,
        error: Option<ContainerExchangeError>,
        exchanges: Vec<(Vec<(DefaultItem, u32)>, Vec<(DefaultItem, u32)>)>,
    }

    impl Container<DefaultItem> for OptedInContainer {
        fn add(&mut self, item: DefaultItem, count: u32) -> u32 {
            self.legacy.add(item, count)
        }
        fn remove(&mut self, item: DefaultItem, count: u32) -> u32 {
            self.legacy.remove(item, count)
        }
        fn count_of(&self, item: DefaultItem) -> u32 {
            self.legacy.count_of(item)
        }
        fn capacity(&self) -> usize {
            self.legacy.capacity()
        }
        fn is_full(&self) -> bool {
            self.legacy.is_full()
        }
        fn iter_slots(
            &self,
        ) -> impl Iterator<Item = Option<&crate::items::InventorySlot<DefaultItem>>> {
            self.legacy.iter_slots()
        }
        fn slot_at(&self, index: usize) -> Option<&crate::items::InventorySlot<DefaultItem>> {
            self.legacy.slot_at(index)
        }
        fn try_exchange_atomic(
            &mut self,
            remove: &[(DefaultItem, u32)],
            add: &[(DefaultItem, u32)],
        ) -> Option<Result<(), ContainerExchangeError>> {
            self.exchanges.push((remove.to_vec(), add.to_vec()));
            if let Some(error) = self.error {
                Some(Err(error))
            } else {
                self.legacy.inventory.try_exchange_atomic(remove, add)
            }
        }
    }

    #[test]
    fn opted_in_hook_gets_normalized_lists_and_bypasses_legacy_mutators() {
        let mut container = OptedInContainer {
            legacy: LegacyContainer::new(1),
            error: None,
            exchanges: vec![],
        };
        container.legacy.inventory.add(DefaultItem::IronOre, 4);
        let registry = registry_for(
            &[(DefaultItem::IronOre, 2), (DefaultItem::IronOre, 2)],
            &[
                (DefaultItem::Crystal, 1),
                (DefaultItem::Empty, 0),
                (DefaultItem::Crystal, 2),
            ],
        );
        assert_eq!(try_craft(&registry, &mut container, 0), Ok(()));
        assert_eq!(
            container.exchanges,
            vec![(
                vec![(DefaultItem::IronOre, 4)],
                vec![(DefaultItem::Crystal, 3)]
            )]
        );
        assert!(container.legacy.removes.is_empty());
        assert!(container.legacy.adds.is_empty());
        assert_eq!(container.count_of(DefaultItem::Crystal), 3);
    }

    #[test]
    fn all_neutral_hook_errors_map_to_the_existing_three_crafting_variants() {
        // Deliberately exhaustive: adding a CraftingError variant breaks compatibility.
        fn exhaustive(error: CraftingError) -> u8 {
            match error {
                CraftingError::InvalidRecipe => 0,
                CraftingError::InsufficientIngredients => 1,
                CraftingError::InventoryFull => 2,
            }
        }
        let registry = registry_for(&[], &[]);
        for (error, expected) in [
            (
                ContainerExchangeError::InvalidRequest,
                CraftingError::InvalidRecipe,
            ),
            (
                ContainerExchangeError::InsufficientItems,
                CraftingError::InsufficientIngredients,
            ),
            (
                ContainerExchangeError::InvalidContainer,
                CraftingError::InventoryFull,
            ),
            (
                ContainerExchangeError::CapacityExceeded,
                CraftingError::InventoryFull,
            ),
        ] {
            let mut container = OptedInContainer {
                legacy: LegacyContainer::new(1),
                error: Some(error),
                exchanges: vec![],
            };
            container.legacy.inventory.add(DefaultItem::IronOre, 1);
            let before = container.legacy.inventory.slots.clone();
            let actual = try_craft(&registry, &mut container, 0).unwrap_err();
            assert_eq!(actual, expected);
            assert_eq!(exhaustive(actual), exhaustive(expected));
            assert_eq!(container.legacy.inventory.slots, before);
            assert!(container.legacy.removes.is_empty());
            assert!(container.legacy.adds.is_empty());
        }
    }

    #[test]
    fn component_crafting_matches_resource_atomicity() {
        let registry = registry_for(&[(DefaultItem::IronOre, 4)], &[(DefaultItem::Crystal, 1)]);
        for count in [3, 4, 5] {
            let mut inv = Inventory::new(1);
            inv.add(DefaultItem::IronOre, count);
            let mut component = ContainerComponent(inv.clone());
            assert_eq!(
                try_craft(&registry, &mut component, 0),
                try_craft(&registry, &mut inv, 0)
            );
            assert_eq!(component.0.slots, inv.slots);
        }
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
    enum MinimalItem {
        #[default]
        DefaultSentinel,
        OtherSentinel,
        Live,
    }

    // No Debug or serde bounds, and Default is not reported by is_empty().
    impl ItemType for MinimalItem {
        fn name(&self) -> &'static str {
            "minimal"
        }
        fn color(&self) -> [f32; 4] {
            [1.0; 4]
        }
        fn to_byte(&self) -> u8 {
            1
        }
        fn from_byte(_: u8) -> Self {
            Self::default()
        }
        fn is_empty(&self) -> bool {
            *self == Self::OtherSentinel
        }
    }

    #[test]
    fn crafting_adds_no_item_bounds_and_rejects_default_or_is_empty_sentinels() {
        let mut inv = Inventory::<MinimalItem>::new(1);
        assert_eq!(inv.add(MinimalItem::Live, 2), 0);
        for sentinel in [MinimalItem::DefaultSentinel, MinimalItem::OtherSentinel] {
            for registry in [
                registry_for(&[(sentinel, 1)], &[]),
                registry_for(&[], &[(sentinel, 1)]),
            ] {
                assert!(!can_craft(&registry, &inv, 0));
                assert_eq!(
                    try_craft(&registry, &mut inv, 0),
                    Err(CraftingError::InvalidRecipe)
                );
                assert_eq!(inv.count_of(MinimalItem::Live), 2);
            }
            let registry = registry_for(
                &[(sentinel, 0), (MinimalItem::Live, 1)],
                &[(sentinel, 0), (MinimalItem::Live, 1)],
            );
            assert!(can_craft(&registry, &inv, 0));
            assert_eq!(try_craft(&registry, &mut inv, 0), Ok(()));
            assert_eq!(inv.count_of(MinimalItem::Live), 2);
        }
    }

    #[test]
    fn invalid_recipe_never_reaches_opted_in_hook_or_legacy_mutators() {
        let registry = registry_for(&[(DefaultItem::IronOre, 1)], &[(DefaultItem::Empty, 1)]);
        let mut container = OptedInContainer {
            legacy: LegacyContainer::new(1),
            error: None,
            exchanges: vec![],
        };
        // Invalid recipe takes precedence over insufficient ingredients.
        assert_eq!(
            try_craft(&registry, &mut container, 0),
            Err(CraftingError::InvalidRecipe)
        );
        assert!(container.exchanges.is_empty());
        assert!(container.legacy.removes.is_empty());
        assert!(container.legacy.adds.is_empty());
    }

    #[test]
    fn output_first_occurrence_order_is_retained_in_builtin_slots() {
        let registry = registry_for(
            &[],
            &[
                (DefaultItem::Wood, 1),
                (DefaultItem::Crystal, 1),
                (DefaultItem::Wood, 2),
                (DefaultItem::Sand, 1),
            ],
        );
        let mut inv = Inventory::new(3);
        assert_eq!(try_craft(&registry, &mut inv, 0), Ok(()));
        let actual: Vec<_> = inv
            .iter_slots()
            .flatten()
            .map(|slot| (slot.item, slot.count))
            .collect();
        assert_eq!(
            actual,
            vec![
                (DefaultItem::Wood, 3),
                (DefaultItem::Crystal, 1),
                (DefaultItem::Sand, 1)
            ]
        );
    }
}
