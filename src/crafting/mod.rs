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
//! enough of every ingredient, consumes them atomically, and produces the
//! outputs. [`can_craft`] performs the read-only check without side effects.
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
use crate::items::inventory::{Container, ContainerComponent, PrimaryContainer};

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
/// types and counts matter.
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
    /// No recipe exists at the given index.
    InvalidRecipe,
    /// The inventory does not hold enough of one or more ingredients.
    InsufficientIngredients,
    /// The inventory does not have room for the produced outputs.
    InventoryFull,
}

/// Returns `true` if the container satisfies all ingredients of `recipe_index`.
///
/// Does **not** mutate the container.
pub fn can_craft<I: ItemType>(
    registry: &CraftingRegistry<I>,
    inventory: &impl Container<I>,
    recipe_index: usize,
) -> bool {
    let Some(recipe) = registry.recipes.get(recipe_index) else {
        return false;
    };
    recipe
        .ingredients
        .iter()
        .all(|ing| inventory.count_of(ing.item) >= ing.count)
}

/// Attempts to craft recipe `recipe_index`, consuming ingredients and producing
/// outputs.
///
/// # Atomicity
///
/// All preconditions are checked before any mutation:
/// - The recipe must exist.
/// - The container must hold enough of every ingredient.
/// - The container must have enough free slots for output item types not yet
///   present.
///
/// Returns `Ok(())` on success and a [`CraftingError`] without modifying the
/// container on failure.
pub fn try_craft<I: ItemType>(
    registry: &CraftingRegistry<I>,
    inventory: &mut impl Container<I>,
    recipe_index: usize,
) -> Result<(), CraftingError> {
    let recipe = registry
        .recipes
        .get(recipe_index)
        .ok_or(CraftingError::InvalidRecipe)?;

    // Pre-check: enough ingredients?
    for ing in &recipe.ingredients {
        if inventory.count_of(ing.item) < ing.count {
            return Err(CraftingError::InsufficientIngredients);
        }
    }

    // Pre-check: free slots for new output types.
    let free_slots = inventory.capacity() - inventory.iter_slots().filter(|s| s.is_some()).count();
    let new_types = recipe
        .outputs
        .iter()
        .filter(|out| inventory.count_of(out.item) == 0)
        .count();
    if new_types > free_slots {
        return Err(CraftingError::InventoryFull);
    }

    // Consume ingredients.
    for ing in &recipe.ingredients {
        inventory.remove(ing.item, ing.count);
    }

    // Produce outputs.
    for out in &recipe.outputs {
        inventory.add(out.item, out.count);
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

fn toggle_crafting_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CraftingMenuState>,
) {
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
            CraftingRecipeRow { recipe_index: usize::MAX },
            Node {
                padding: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
            Text::new("── Crafting (C to close) ──"),
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(root).add_child(header);

    if registry.recipes.is_empty() {
        let empty = commands
            .spawn((
                CraftingRecipeRow { recipe_index: usize::MAX },
                Text::new("No recipes registered."),
                TextFont { font_size: 13.0, ..default() },
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

        let label = format!("[{}]  {} → {}", recipe.name, ing_str, out_str);

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
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgb(0.70, 0.70, 0.70)),
            ))
            .id();
        commands.entity(root).add_child(row);
    }

    // Footer hint.
    let footer = commands
        .spawn((
            CraftingRecipeRow { recipe_index: usize::MAX },
            Node {
                padding: UiRect::top(Val::Px(6.0)),
                ..default()
            },
            Text::new("Green = craftable  |  click to craft"),
            TextFont { font_size: 11.0, ..default() },
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
            (true, true)  => BackgroundColor(Color::srgba(0.18, 0.50, 0.18, 0.95)),
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
/// Mirrors the [`MiningPlugin`] / [`MiningInputPlugin`] split pattern.
pub struct CraftingPlugin<I: ItemType> {
    _m: PhantomData<I>,
}

impl<I: ItemType> CraftingPlugin<I> {
    pub fn new() -> Self {
        Self { _m: PhantomData }
    }
}

impl<I: ItemType> Default for CraftingPlugin<I> {
    fn default() -> Self { Self::new() }
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
    fn default() -> Self { Self::new() }
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
            ingredients: vec![CraftingIngredient { item: DefaultItem::IronOre, count: 4 }],
            outputs: vec![CraftingIngredient { item: DefaultItem::Crystal, count: 1 }],
        });
        r.add(CraftingRecipe {
            name: "Rare from Ore+Crystal",
            ingredients: vec![
                CraftingIngredient { item: DefaultItem::IronOre, count: 2 },
                CraftingIngredient { item: DefaultItem::Crystal, count: 1 },
            ],
            outputs: vec![CraftingIngredient { item: DefaultItem::RareMineral, count: 1 }],
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
        // Fill all slots with RockFragment so no room for Crystal output.
        let mut inv = Inventory::new(1);
        inv.add(DefaultItem::IronOre, 4); // occupies the 1 slot

        // Now inventory is full (1 slot used).
        // Crystal is a new type — needs a free slot, but none available.
        let result = try_craft(&registry, &mut inv, 0);
        assert_eq!(result, Err(CraftingError::InventoryFull));
        // Ingredients not consumed.
        assert_eq!(inv.count_of(DefaultItem::IronOre), 4);
    }
}
