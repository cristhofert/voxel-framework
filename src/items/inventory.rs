//! Inventory resource and the `ItemType` trait.
//!
//! [`ItemType`] mirrors [`crate::voxel::VoxelType`] — implement it for any
//! `Copy + Hash` enum to make it usable as an inventory item.

use std::hash::Hash;

use bevy::prelude::*;

// ─── ItemType trait ───────────────────────────────────────────────────────────

/// Core trait that all item types must implement.
///
/// Mirrors [`crate::voxel::VoxelType`]: implementors must be lightweight value
/// types (typically unit-variant enums) so they can be stored densely in
/// inventory slots.
pub trait ItemType:
    Copy + Clone + Default + Send + Sync + PartialEq + Eq + Hash + 'static
{
    /// Human-readable name for this item (e.g. `"Iron Ore"`).
    fn name(&self) -> &'static str;

    /// Default RGBA display color for this item.
    fn color(&self) -> [f32; 4];

    /// Serialises this item to a single byte for binary persistence.
    fn to_byte(&self) -> u8;

    /// Deserialises an item from a single byte. Unknown bytes should map to
    /// `Self::default()`.
    fn from_byte(b: u8) -> Self;

    /// Returns `true` if this item represents the "empty slot" sentinel.
    ///
    /// Override this for an enum variant that means "nothing here".
    fn is_empty(&self) -> bool {
        false
    }
}

// ─── InventorySlot ────────────────────────────────────────────────────────────

/// A single inventory slot holding a stack of one item type.
#[derive(Debug, Clone, PartialEq)]
pub struct InventorySlot<I: ItemType> {
    /// The item stored in this slot.
    pub item: I,
    /// Number of items in the stack.
    pub count: u32,
}

// ─── Inventory ────────────────────────────────────────────────────────────────

/// Fixed-capacity player inventory.
///
/// Each slot holds an optional [`InventorySlot`]. Multiple stacks of the same
/// item type are stored in separate slots; [`add`][Self::add] fills existing
/// stacks before opening new ones.
#[derive(Resource, Debug, Clone)]
pub struct Inventory<I: ItemType> {
    /// The slots in this inventory. `None` means the slot is empty.
    pub slots: Vec<Option<InventorySlot<I>>>,
    /// Maximum number of slots.
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

// ─── Messages ────────────────────────────────────────────────────────────────

/// Emitted when one or more items are successfully added to the inventory.
#[derive(Message, Debug, Clone)]
pub struct ItemPickedUp<I: ItemType> {
    /// The item type that was picked up.
    pub item: I,
    /// How many were added.
    pub count: u32,
}

// ─── DefaultItem ─────────────────────────────────────────────────────────────

/// The default set of item types bundled with the framework.
///
/// Mirrors [`crate::voxel::DefaultVoxel`]: `Empty` is the sentinel, the rest
/// correspond to common mineral drops.
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
    /// Empty slot sentinel — not a real item.
    #[default]
    Empty,
    /// Fragment of common rock.
    RockFragment,
    /// Raw iron ore chunk.
    IronOre,
    /// Silicon crystal shard.
    Crystal,
    /// Rare mineral node.
    RareMineral,
}

impl ItemType for DefaultItem {
    fn name(&self) -> &'static str {
        match self {
            DefaultItem::Empty => "Empty",
            DefaultItem::RockFragment => "Rock Fragment",
            DefaultItem::IronOre => "Iron Ore",
            DefaultItem::Crystal => "Crystal",
            DefaultItem::RareMineral => "Rare Mineral",
        }
    }

    fn color(&self) -> [f32; 4] {
        match self {
            DefaultItem::Empty => [0.0, 0.0, 0.0, 0.0],
            DefaultItem::RockFragment => [0.35, 0.30, 0.28, 1.0],
            DefaultItem::IronOre => [0.55, 0.35, 0.20, 1.0],
            DefaultItem::Crystal => [0.40, 0.50, 0.80, 1.0],
            DefaultItem::RareMineral => [0.20, 0.85, 0.60, 1.0],
        }
    }

    fn to_byte(&self) -> u8 {
        match self {
            DefaultItem::Empty => 0,
            DefaultItem::RockFragment => 1,
            DefaultItem::IronOre => 2,
            DefaultItem::Crystal => 3,
            DefaultItem::RareMineral => 4,
        }
    }

    fn from_byte(b: u8) -> Self {
        match b {
            1 => DefaultItem::RockFragment,
            2 => DefaultItem::IronOre,
            3 => DefaultItem::Crystal,
            4 => DefaultItem::RareMineral,
            _ => DefaultItem::Empty,
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, DefaultItem::Empty)
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
}
