//! Inventory serialisation to/from JSON on disk.
//!
//! Uses `serde_json` for a human-readable format that is easy to inspect and
//! edit. The on-disk format is a JSON object with `capacity` and `slots` fields.

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

/// Serialises `inventory` to a JSON file at `path`.
///
/// Creates the file (and any parent directories) if they do not exist.
///
/// # Errors
///
/// Returns an error if the file cannot be created or the JSON cannot be
/// written.
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

/// Deserialises an [`Inventory`] from the JSON file at `path`.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the JSON is malformed.
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
