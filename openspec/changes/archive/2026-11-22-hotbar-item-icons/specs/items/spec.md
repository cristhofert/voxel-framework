# Hotbar content requirements

- ItemIconPalette<I> MUST explicitly associate an item with RGBA or a voxel's
  color; equality MUST NOT depend on shared item/voxel byte ids.
- Occupied slots MUST render a padded square or '?' fallback and every count,
  including one. Slot n MUST read container slot n without compacting empties.
- The selected item name MUST appear above the bar; empty, missing, or ambiguous
  content MUST display "Empty hands". Content/selection updates MUST NOT mutate
  inventory and MUST clear stale UI when a container disappears or is replaced.
- Legacy HotbarUiPlugin MUST still work without content or an inventory.
- Fourteen slots MUST fit one bottom-centered row, at most 50px per slot with
  default 4px gaps and at least 16px screen margins. Resize MUST update layout.
- Keys 1-9 and wheel wraparound MUST retain existing behavior across all slots.
- Native verification MUST cover 800x600, 1280x720, 1920x1080; checks and actual
  limitations MUST be recorded before requesting release authorization.
