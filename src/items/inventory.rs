//! Slot-based inventory resource and the [`ItemType`] trait.
//!
//! This module is the data layer of the item system.  It defines the
//! [`ItemType`] trait that all item enums must implement, the [`Inventory`]
//! resource that stores collected items, and the [`ItemPickedUp`] message
//! emitted whenever items successfully enter the inventory.
//!
//! The design mirrors [`crate::voxel::VoxelType`]: item types are lightweight
//! value types (typically unit-variant enums) stored densely in a flat slot
//! vector.  The actual spawning, attraction, and pickup logic lives in
//! [`crate::items::drops`]; this module is concerned only with the data model.

use std::hash::Hash;

use bevy::prelude::*;

// ─── Container trait ─────────────────────────────────────────────────────────

/// Abstraction over slot-based item storage.
///
/// Any type that holds items in indexed slots can implement this trait to be
/// usable with the framework's pickup, hotbar, and crafting systems.
///
/// The framework provides two built-in implementors:
/// - [`Inventory<I>`] — the standalone resource form (backward-compatible).
/// - [`ContainerComponent<I>`] — the Bevy component form used by the new
///   entity-based pipeline.
pub trait Container<I: ItemType> {
    /// Adds `count` of `item`.  Returns the number of items that could NOT be
    /// inserted due to capacity (0 = all inserted).
    fn add(&mut self, item: I, count: u32) -> u32;

    /// Removes up to `count` of `item`.  Returns the number actually removed.
    fn remove(&mut self, item: I, count: u32) -> u32;

    /// Total number of `item` across all slots.
    fn count_of(&self, item: I) -> u32;

    /// Maximum number of distinct item stacks this container can hold.
    fn capacity(&self) -> usize;

    /// Returns `true` if every slot is occupied.
    fn is_full(&self) -> bool;

    /// Iterates over all slots, yielding `Some(&InventorySlot<I>)` for occupied
    /// slots and `None` for empty ones.  Yields exactly `capacity()` items.
    fn iter_slots(&self) -> impl Iterator<Item = Option<&InventorySlot<I>>>;

    /// Returns the slot at `index`, or `None` if the index is out of bounds or
    /// the slot is empty.  Never panics.
    fn slot_at(&self, index: usize) -> Option<&InventorySlot<I>>;
}

// ─── ItemType trait ───────────────────────────────────────────────────────────

/// Contract that all item types must satisfy to participate in the item pipeline.
///
/// Implement this trait for any `Copy + Hash + Default` enum to make it usable
/// with [`Inventory`], [`crate::items::drops::DropTable`],
/// [`crate::items::drops::DropsPlugin`], and
/// [`crate::items::persistence::save_inventory`] /
/// [`crate::items::persistence::load_inventory`].
///
/// The framework ships [`DefaultItem`] as a ready-made implementation.
/// Custom implementations should designate one variant as the *empty sentinel*
/// (returned by [`Default`]) and override [`ItemType::is_empty`] accordingly.
///
/// # Contract
///
/// * `Copy + Clone` — slots are copied freely; no heap allocation per item.
/// * `Default` — produces the empty-sentinel value.
/// * `PartialEq + Eq + Hash` — required for slot deduplication and hash-map
///   lookups in [`crate::items::drops::DropTable`].
/// * `Send + Sync + 'static` — required for use as a Bevy [`Resource`] type
///   parameter.
/// * [`to_byte`](ItemType::to_byte) / [`from_byte`](ItemType::from_byte) —
///   stable single-byte encoding used by the JSON persistence layer.  Unknown
///   bytes must round-trip to `Self::default()`.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::ItemType;
///
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
/// enum MyItem { #[default] Empty, Wood, Stone }
///
/// impl ItemType for MyItem {
///     fn name(&self) -> &'static str {
///         match self { MyItem::Empty => "Empty", MyItem::Wood => "Wood", MyItem::Stone => "Stone" }
///     }
///     fn color(&self) -> [f32; 4] {
///         match self {
///             MyItem::Empty => [0.0; 4],
///             MyItem::Wood  => [0.55, 0.35, 0.20, 1.0],
///             MyItem::Stone => [0.50, 0.50, 0.50, 1.0],
///         }
///     }
///     fn to_byte(&self) -> u8   { match self { MyItem::Empty => 0, MyItem::Wood => 1, MyItem::Stone => 2 } }
///     fn from_byte(b: u8) -> Self { match b { 1 => MyItem::Wood, 2 => MyItem::Stone, _ => MyItem::Empty } }
///     fn is_empty(&self) -> bool { matches!(self, MyItem::Empty) }
/// }
/// ```
pub trait ItemType:
    Copy + Clone + Default + Send + Sync + PartialEq + Eq + Hash + 'static
{
    /// Human-readable display name for this item (e.g. `"Iron Ore"`).
    ///
    /// Used by UI widgets such as the pickup notification overlay.
    fn name(&self) -> &'static str;

    /// Default RGBA display colour for this item, with components in `[0.0, 1.0]`.
    ///
    /// Used by hotbar and inventory UI as a fallback when no sprite is available.
    fn color(&self) -> [f32; 4];

    /// Encodes this item variant as a single byte for JSON persistence.
    ///
    /// The mapping must be stable across game versions; changing it will
    /// corrupt existing save files.  The empty-sentinel variant should map
    /// to `0`.
    fn to_byte(&self) -> u8;

    /// Decodes a single byte back into an item variant.
    ///
    /// Unknown byte values — for example from a future game version — **must**
    /// return `Self::default()` rather than panicking so that save files from
    /// newer versions degrade gracefully.
    fn from_byte(b: u8) -> Self;

    /// Returns `true` if this variant is the empty-slot sentinel.
    ///
    /// The default implementation returns `false`; override it for the variant
    /// that represents "nothing here" (typically the `Default` variant).
    fn is_empty(&self) -> bool {
        false
    }

    /// Tool level provided when this item is the active hotbar selection.
    ///
    /// `0` = bare hands. Matched against [`crate::voxel::VoxelType::mining_level`]:
    /// if `tool_level < voxel.mining_level()`, mining makes no progress.
    ///
    /// Default: `0`.
    fn tool_level(&self) -> u8 {
        0
    }

    /// Mining speed multiplier when this item is the active tool.
    ///
    /// The effective break time is divided by this value (`2.0` = twice as fast).
    /// Default: `1.0`.
    fn tool_speed(&self) -> f32 {
        1.0
    }
}

// ─── InventorySlot ────────────────────────────────────────────────────────────

/// A single occupied inventory slot, holding a stack of one item type.
///
/// Slots are always non-empty — "no item" is represented by `None` at the
/// [`Inventory::slots`] level rather than a zero-count `InventorySlot`.
/// The `count` field is therefore always `>= 1` in practice; the drop systems
/// never produce a zero-count slot.
#[derive(Debug, Clone, PartialEq)]
pub struct InventorySlot<I: ItemType> {
    /// The item type stored in this slot.
    pub item: I,
    /// Number of items in the stack.  Always `>= 1` for a live slot.
    pub count: u32,
}

// ─── Inventory ────────────────────────────────────────────────────────────────

/// Fixed-capacity player inventory backed by a flat slot vector.
///
/// The inventory holds up to `capacity` stacks, each stored as
/// `Option<`[`InventorySlot`]`<I>>`.  `None` denotes an empty slot.
///
/// Items of the same type are automatically stacked: [`Inventory::add`]
/// fills existing stacks of the matching type before opening a new slot.
/// [`Inventory::remove`] drains stacks in order and clears slots that reach
/// zero.
///
/// The default capacity is 36 slots (4 rows × 9 columns), matching the
/// standard Minecraft-style layout expected by the built-in hotbar and
/// inventory UI widgets.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::{Inventory, DefaultItem};
///
/// let mut inv = Inventory::<DefaultItem>::new(9);
/// inv.add(DefaultItem::IronOre, 5);
/// assert_eq!(inv.count_of(DefaultItem::IronOre), 5);
/// ```
#[derive(Resource, Debug, Clone)]
pub struct Inventory<I: ItemType> {
    /// The backing slot vector.  `None` at index `i` means slot `i` is empty.
    ///
    /// The length always equals `capacity`; slots are never removed from the
    /// vector.
    pub slots: Vec<Option<InventorySlot<I>>>,
    /// Maximum number of distinct item stacks the inventory can hold.
    ///
    /// Set at construction time; cannot be changed after the fact without
    /// rebuilding the inventory.
    pub capacity: usize,
}

impl<I: ItemType> Inventory<I> {
    /// Creates a new empty inventory with the given slot capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: vec![None; capacity],
            capacity,
        }
    }

    /// Adds `count` of `item` to the inventory.
    ///
    /// Items are stacked onto existing slots of the same type first, then into
    /// empty slots. Returns the number of items that could **not** be added
    /// because the inventory is full.
    pub fn add(&mut self, item: I, count: u32) -> u32 {
        let mut remaining = count;

        // First pass: fill existing stacks.
        for slot in self.slots.iter_mut().flatten() {
            if remaining == 0 {
                break;
            }
            if slot.item == item {
                slot.count += remaining;
                remaining = 0;
            }
        }

        // Second pass: open new slots for leftovers.
        if remaining > 0 {
            for slot in self.slots.iter_mut() {
                if remaining == 0 {
                    break;
                }
                if slot.is_none() {
                    *slot = Some(InventorySlot { item, count: remaining });
                    remaining = 0;
                }
            }
        }

        remaining
    }

    /// Removes up to `count` of `item` from the inventory.
    ///
    /// Returns the number of items that were actually removed (may be less than
    /// `count` if the inventory held fewer).
    pub fn remove(&mut self, item: I, count: u32) -> u32 {
        let mut to_remove = count;
        let mut removed = 0u32;

        for slot in self.slots.iter_mut() {
            if to_remove == 0 {
                break;
            }
            if let Some(s) = slot {
                if s.item == item {
                    let take = s.count.min(to_remove);
                    s.count -= take;
                    to_remove -= take;
                    removed += take;
                    if s.count == 0 {
                        *slot = None;
                    }
                }
            }
        }

        removed
    }

    /// Returns the total number of `item` across all slots.
    pub fn count_of(&self, item: I) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum()
    }

    /// Returns `true` if every slot is occupied.
    pub fn is_full(&self) -> bool {
        self.slots.iter().all(|s| s.is_some())
    }
}

impl<I: ItemType> Default for Inventory<I> {
    /// Creates a default inventory with 36 slots (4 rows × 9 columns).
    fn default() -> Self {
        Self::new(36)
    }
}

// ─── Container impl for Inventory ────────────────────────────────────────────

impl<I: ItemType> Container<I> for Inventory<I> {
    fn add(&mut self, item: I, count: u32) -> u32 {
        Inventory::add(self, item, count)
    }

    fn remove(&mut self, item: I, count: u32) -> u32 {
        Inventory::remove(self, item, count)
    }

    fn count_of(&self, item: I) -> u32 {
        Inventory::count_of(self, item)
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn is_full(&self) -> bool {
        Inventory::is_full(self)
    }

    fn iter_slots(&self) -> impl Iterator<Item = Option<&InventorySlot<I>>> {
        self.slots.iter().map(|s| s.as_ref())
    }

    fn slot_at(&self, index: usize) -> Option<&InventorySlot<I>> {
        self.slots.get(index).and_then(|s| s.as_ref())
    }
}

// ─── ContainerComponent ───────────────────────────────────────────────────────

/// Bevy [`Component`] newtype that wraps an [`Inventory<I>`] for use in the
/// entity-based item pipeline.
///
/// Add this alongside [`PrimaryContainer`] on the player entity so the magnet,
/// hotbar, and crafting systems can locate the player's inventory via ECS
/// queries instead of a global resource.
///
/// # Examples
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use voxel_framework::items::{ContainerComponent, PrimaryContainer, DefaultItem};
///
/// fn spawn_player(mut commands: Commands) {
///     commands.spawn((
///         ContainerComponent::<DefaultItem>::new(36),
///         PrimaryContainer,
///     ));
/// }
/// ```
#[derive(Component)]
pub struct ContainerComponent<I: ItemType>(pub Inventory<I>);

impl<I: ItemType> ContainerComponent<I> {
    /// Creates a new `ContainerComponent` wrapping a fresh [`Inventory`] with
    /// the given `capacity`.
    pub fn new(capacity: usize) -> Self {
        Self(Inventory::new(capacity))
    }
}

impl<I: ItemType> Container<I> for ContainerComponent<I> {
    fn add(&mut self, item: I, count: u32) -> u32 {
        self.0.add(item, count)
    }

    fn remove(&mut self, item: I, count: u32) -> u32 {
        self.0.remove(item, count)
    }

    fn count_of(&self, item: I) -> u32 {
        self.0.count_of(item)
    }

    fn capacity(&self) -> usize {
        self.0.capacity
    }

    fn is_full(&self) -> bool {
        self.0.is_full()
    }

    fn iter_slots(&self) -> impl Iterator<Item = Option<&InventorySlot<I>>> {
        self.0.slots.iter().map(|s| s.as_ref())
    }

    fn slot_at(&self, index: usize) -> Option<&InventorySlot<I>> {
        self.0.slots.get(index).and_then(|s| s.as_ref())
    }
}

// ─── PrimaryContainer marker ──────────────────────────────────────────────────

/// Zero-size marker component that identifies the entity whose
/// [`ContainerComponent`] is the player's primary backpack.
///
/// Exactly one entity should carry this marker at runtime.  If zero or more
/// than one exist, the pickup and hotbar systems skip silently — no panic.
#[derive(Component, Default)]
pub struct PrimaryContainer;

// ─── Messages ────────────────────────────────────────────────────────────────

/// Message emitted when one or more items are successfully added to the inventory.
///
/// The magnet system in [`crate::items::drops`] writes this message after
/// adding items to [`Inventory`].  UI systems — such as the pickup notification
/// overlay — subscribe to this message to display transient feedback.
///
/// Note: this message is only written when at least one item was actually
/// inserted (i.e. `count > 0`).  If the inventory was full the message is
/// **not** written even though a pickup was attempted.
#[derive(Message, Debug, Clone)]
pub struct ItemPickedUp<I: ItemType> {
    /// The item type that was picked up.
    pub item: I,
    /// Number of items that were successfully inserted.  Always `>= 1`.
    pub count: u32,
}

// ─── DefaultItem ─────────────────────────────────────────────────────────────

/// Built-in item type covering common mineral pickups.
///
/// `DefaultItem` is the item counterpart of [`crate::voxel::DefaultVoxel`]:
/// it implements [`ItemType`] out of the box and is wired into the default
/// [`crate::items::drops::DropTable`] entries provided by the framework.
///
/// `Empty` is the empty-slot sentinel returned by [`Default`] and by
/// [`ItemType::from_byte`] for unknown byte values.  It should never appear
/// as a real collectible.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::{DefaultItem, Inventory};
///
/// let mut inv = Inventory::<DefaultItem>::new(9);
/// inv.add(DefaultItem::Crystal, 3);
/// ```
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum DefaultItem {
    /// Empty slot sentinel — not a real item; returned by [`Default`].
    #[default]
    Empty,
    /// Fragment of common rock; drops from basic stone voxels.
    RockFragment,
    /// Raw iron ore chunk; drops from iron-bearing voxels.
    IronOre,
    /// Silicon crystal shard; drops from crystal voxels.
    Crystal,
    /// Rare mineral node; drops from high-value voxels.
    RareMineral,
    /// Wooden pickaxe — tool level 1, normal speed. Crafted from wood.
    WoodenPickaxe,
    /// Stone pickaxe — tool level 1, 2× speed. Crafted from rock fragments.
    StonePickaxe,
    /// Iron pickaxe — tool level 2, 4× speed. Crafted from iron ore.
    IronPickaxe,
}

impl ItemType for DefaultItem {
    fn name(&self) -> &'static str {
        match self {
            DefaultItem::Empty => "Empty",
            DefaultItem::RockFragment => "Rock Fragment",
            DefaultItem::IronOre => "Iron Ore",
            DefaultItem::Crystal => "Crystal",
            DefaultItem::RareMineral => "Rare Mineral",
            DefaultItem::WoodenPickaxe => "Wooden Pickaxe",
            DefaultItem::StonePickaxe => "Stone Pickaxe",
            DefaultItem::IronPickaxe => "Iron Pickaxe",
        }
    }

    fn color(&self) -> [f32; 4] {
        match self {
            DefaultItem::Empty => [0.0, 0.0, 0.0, 0.0],
            DefaultItem::RockFragment => [0.35, 0.30, 0.28, 1.0],
            DefaultItem::IronOre => [0.55, 0.35, 0.20, 1.0],
            DefaultItem::Crystal => [0.40, 0.50, 0.80, 1.0],
            DefaultItem::RareMineral => [0.20, 0.85, 0.60, 1.0],
            DefaultItem::WoodenPickaxe => [0.55, 0.35, 0.20, 1.0],
            DefaultItem::StonePickaxe => [0.50, 0.50, 0.50, 1.0],
            DefaultItem::IronPickaxe => [0.70, 0.75, 0.80, 1.0],
        }
    }

    fn to_byte(&self) -> u8 {
        match self {
            DefaultItem::Empty => 0,
            DefaultItem::RockFragment => 1,
            DefaultItem::IronOre => 2,
            DefaultItem::Crystal => 3,
            DefaultItem::RareMineral => 4,
            DefaultItem::WoodenPickaxe => 5,
            DefaultItem::StonePickaxe => 6,
            DefaultItem::IronPickaxe => 7,
        }
    }

    fn from_byte(b: u8) -> Self {
        match b {
            1 => DefaultItem::RockFragment,
            2 => DefaultItem::IronOre,
            3 => DefaultItem::Crystal,
            4 => DefaultItem::RareMineral,
            5 => DefaultItem::WoodenPickaxe,
            6 => DefaultItem::StonePickaxe,
            7 => DefaultItem::IronPickaxe,
            _ => DefaultItem::Empty,
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, DefaultItem::Empty)
    }

    fn tool_level(&self) -> u8 {
        match self {
            DefaultItem::WoodenPickaxe => 1,
            DefaultItem::StonePickaxe  => 1,
            DefaultItem::IronPickaxe   => 2,
            _ => 0,
        }
    }

    fn tool_speed(&self) -> f32 {
        match self {
            DefaultItem::WoodenPickaxe => 1.0,
            DefaultItem::StonePickaxe  => 2.0,
            DefaultItem::IronPickaxe   => 4.0,
            _ => 1.0,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── add ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_inventory_add_and_count() {
        let mut inv = Inventory::<DefaultItem>::new(9);
        let leftover = inv.add(DefaultItem::IronOre, 5);
        assert_eq!(leftover, 0);
        assert_eq!(inv.count_of(DefaultItem::IronOre), 5);
    }

    #[test]
    fn test_inventory_add_stacks_same_type() {
        let mut inv = Inventory::<DefaultItem>::new(9);
        inv.add(DefaultItem::Crystal, 3);
        inv.add(DefaultItem::Crystal, 7);
        assert_eq!(inv.count_of(DefaultItem::Crystal), 10);
        // Should occupy only one slot.
        let filled = inv.slots.iter().filter(|s| s.is_some()).count();
        assert_eq!(filled, 1);
    }

    // ── remove ───────────────────────────────────────────────────────────────

    #[test]
    fn test_inventory_remove() {
        let mut inv = Inventory::<DefaultItem>::new(9);
        inv.add(DefaultItem::RockFragment, 10);
        let removed = inv.remove(DefaultItem::RockFragment, 4);
        assert_eq!(removed, 4);
        assert_eq!(inv.count_of(DefaultItem::RockFragment), 6);
    }

    #[test]
    fn test_inventory_remove_more_than_held_returns_actual() {
        let mut inv = Inventory::<DefaultItem>::new(9);
        inv.add(DefaultItem::RareMineral, 3);
        let removed = inv.remove(DefaultItem::RareMineral, 10);
        assert_eq!(removed, 3);
        assert_eq!(inv.count_of(DefaultItem::RareMineral), 0);
    }

    #[test]
    fn test_inventory_remove_clears_slot() {
        let mut inv = Inventory::<DefaultItem>::new(9);
        inv.add(DefaultItem::IronOre, 2);
        inv.remove(DefaultItem::IronOre, 2);
        assert!(inv.slots[0].is_none());
    }

    // ── full ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_inventory_full() {
        let mut inv = Inventory::<DefaultItem>::new(2);
        inv.add(DefaultItem::IronOre, 1);
        inv.add(DefaultItem::Crystal, 1);
        assert!(inv.is_full());

        // Adding more should return a leftover.
        let leftover = inv.add(DefaultItem::RockFragment, 5);
        assert_eq!(leftover, 5);
    }

    // ── Container trait via Inventory ─────────────────────────────────────────

    #[test]
    fn container_trait_add_and_count() {
        let mut inv = Inventory::<DefaultItem>::new(9);
        let leftover = Container::add(&mut inv, DefaultItem::IronOre, 5);
        assert_eq!(leftover, 0);
        assert_eq!(Container::count_of(&inv, DefaultItem::IronOre), 5);
    }

    #[test]
    fn container_slot_at_out_of_bounds_returns_none() {
        let inv = Inventory::<DefaultItem>::new(9);
        assert!(Container::slot_at(&inv, 100).is_none());
    }

    #[test]
    fn container_iter_slots_yields_capacity_items() {
        let inv = Inventory::<DefaultItem>::new(9);
        assert_eq!(Container::iter_slots(&inv).count(), 9);
    }

    // ── ContainerComponent ───────────────────────────────────────────────────

    #[test]
    fn container_component_delegates_correctly() {
        let mut cc = ContainerComponent::<DefaultItem>::new(9);
        cc.add(DefaultItem::Crystal, 3);
        assert_eq!(cc.count_of(DefaultItem::Crystal), 3);
        assert_eq!(cc.0.count_of(DefaultItem::Crystal), 3);
    }

    #[test]
    fn container_component_slot_at_oob_no_panic() {
        let cc = ContainerComponent::<DefaultItem>::new(9);
        assert!(cc.slot_at(100).is_none());
    }
}
