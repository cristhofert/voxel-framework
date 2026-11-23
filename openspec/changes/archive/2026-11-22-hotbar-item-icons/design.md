# Design

Keep HotbarUiPlugin and its configuration/source paths compatible. Move its
implementation into ui::hotbar. Add HotbarContentPlugin<I> as an explicit
extension (one item type per hotbar) and ItemIconPalette<I> with register,
register_voxel, color, remove. Full item equality keys the palette; no id mapping.

Read the unique PrimaryContainer + ContainerComponent<I> directly each frame.
Only derived Bevy UI components retain presentation; no inventory mirror.
Use slot_at(index), name(), and count including one; unknown items render '?'.
Publish in PostUpdate before Bevy UI preparation so Update input/inventory writes
appear together. Missing/ambiguous containers resolve to empty state.

Retain one centered bottom row, 16px horizontal margins, default 4px gaps,
square slots capped at 50px for content mode. A PostUpdate sizing system uses
the primary Window logical width / UiScale to set both dimensions before UI
layout. Flex shrinking alone kept height at 50px, so native regression evidence
required explicit two-dimensional sizing. Extremely narrow windows shrink gaps.
Legacy configuration stays available. Slot count is configured before Startup.
Register palette in Startup; publication reads it after all Startup systems.

Use deterministic real-App component assertions plus native computed layout and
screenshots at 800x600, 1280x720, 1920x1080. Keep Cargo cache sources untouched;
all Cargo operations offline and existing artifacts reused.
