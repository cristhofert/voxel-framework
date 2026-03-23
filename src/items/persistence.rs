//! Inventory serialisation and deserialisation to/from JSON files on disk.
//!
//! This module provides two free functions — [`save_inventory`] and
//! [`load_inventory`] — that convert an [`crate::items::inventory::Inventory`]
//! to and from a human-readable JSON file.  The format is intentionally simple
//! and hand-editable.
//!
//! # File format
//!
//! The JSON file is a single object with two top-level fields:
//!
//! ```json
//! {
//!   "capacity": 36,
//!   "slots": [
//!     null,
//!     { "item_byte": 2, "count": 7 },
//!     null
//!   ]
//! }
//! ```
//!
//! * `capacity` — the maximum slot count of the original inventory.
//! * `slots` — one entry per slot; `null` for empty slots, or an object with
//!   `item_byte` (the value returned by [`crate::items::inventory::ItemType::to_byte`])
//!   and `count`.
//!
//! # Save / load contract
//!
//! * [`save_inventory`] creates the file and any missing parent directories.
//! * [`load_inventory`] restores the inventory with the same `capacity` and
//!   slot layout.  Unknown `item_byte` values are silently replaced with
//!   `I::default()` (the empty-slot sentinel), ensuring forward compatibility.
//! * Both functions propagate I/O and JSON errors as `Box<dyn std::error::Error>`.

use serde::{Deserialize, Serialize};

use super::inventory::{Inventory, InventorySlot, ItemType};

// ─── On-disk format ──────────────────────────────────────────────────────────

/// Serialisable representation of an [`InventorySlot`].
#[derive(Serialize, Deserialize)]
struct SlotDto {
    item_byte: u8,
    count: u32,
}

/// Serialisable representation of an [`Inventory`].
#[derive(Serialize, Deserialize)]
struct InventoryDto {
    capacity: usize,
    slots: Vec<Option<SlotDto>>,
}

// ─── save_inventory ───────────────────────────────────────────────────────────

/// Writes `inventory` to a pretty-printed JSON file at `path`.
///
/// Creates the file and any missing parent directories.  If the file already
/// exists it is overwritten atomically (via [`std::fs::write`]).
///
/// # Parameters
///
/// * `inventory` — the [`crate::items::inventory::Inventory`] to serialise.
/// * `path` — filesystem path for the output file (e.g. `"saves/inventory.json"`).
///
/// # Errors
///
/// Returns a boxed error if:
/// * A parent directory cannot be created ([`std::io::Error`]).
/// * The JSON cannot be serialised ([`serde_json::Error`]).
/// * The file cannot be written ([`std::io::Error`]).
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::{DefaultItem, Inventory, save_inventory};
///
/// let mut inv = Inventory::<DefaultItem>::new(36);
/// inv.add(DefaultItem::IronOre, 5);
/// save_inventory(&inv, "saves/player.json").expect("failed to save inventory");
/// ```
pub fn save_inventory<I: ItemType + Serialize>(
    inventory: &Inventory<I>,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let dto = InventoryDto {
        capacity: inventory.capacity,
        slots: inventory
            .slots
            .iter()
            .map(|slot| {
                slot.as_ref().map(|s| SlotDto {
                    item_byte: s.item.to_byte(),
                    count: s.count,
                })
            })
            .collect(),
    };

    // Ensure parent directory exists.
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let json = serde_json::to_string_pretty(&dto)?;
    std::fs::write(path, json)?;
    Ok(())
}

// ─── load_inventory ───────────────────────────────────────────────────────────

/// Reads and deserialises an [`crate::items::inventory::Inventory`] from the
/// JSON file at `path`.
///
/// Unknown `item_byte` values in the file are decoded as `I::default()` (the
/// empty-slot sentinel), so save files from newer game versions load without
/// errors — the unknown items simply become empty slots.
///
/// # Parameters
///
/// * `path` — filesystem path of the JSON file written by [`save_inventory`].
///
/// # Errors
///
/// Returns a boxed error if:
/// * The file does not exist or cannot be read ([`std::io::Error`]).
/// * The file contents are not valid JSON or do not match the expected schema
///   ([`serde_json::Error`]).
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::{DefaultItem, load_inventory};
///
/// let inv = load_inventory::<DefaultItem>("saves/player.json")
///     .expect("failed to load inventory");
/// println!("iron ore: {}", inv.count_of(DefaultItem::IronOre));
/// ```
pub fn load_inventory<I: ItemType + for<'de> Deserialize<'de>>(
    path: &str,
) -> Result<Inventory<I>, Box<dyn std::error::Error>> {
    let json = std::fs::read_to_string(path)?;
    let dto: InventoryDto = serde_json::from_str(&json)?;

    let slots = dto
        .slots
        .into_iter()
        .map(|slot| {
            slot.map(|s| InventorySlot {
                item: I::from_byte(s.item_byte),
                count: s.count,
            })
        })
        .collect();

    Ok(Inventory {
        capacity: dto.capacity,
        slots,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::DefaultItem;

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut inv = Inventory::<DefaultItem>::new(9);
        inv.add(DefaultItem::IronOre, 7);
        inv.add(DefaultItem::Crystal, 3);

        let path = "/tmp/voxel_framework_test_inventory.json";
        save_inventory(&inv, path).expect("save failed");

        let loaded = load_inventory::<DefaultItem>(path).expect("load failed");
        assert_eq!(loaded.capacity, 9);
        assert_eq!(loaded.count_of(DefaultItem::IronOre), 7);
        assert_eq!(loaded.count_of(DefaultItem::Crystal), 3);

        // Cleanup.
        let _ = std::fs::remove_file(path);
    }
}
