# Drop lifecycle and optional visuals

## Intent
Consumers need collectible drops that expire without a player and a reusable visible default, without implementing framework behavior in the game. Existing positioned spawn, magnet motion, full rejection, and partial pickup remain compatible.

## Scope
Fix collector-independent expiry in `src/items/drops.rs`; add an items-gated `DropVisualPlugin<I>` as Default Implementation; re-export it and supply a graphical example and integration instructions. No changes to game repositories, persistence, item traits, convenience bundles, dependencies, releases, or multiplayer pickup policy.

## Consumer impact
Existing DropsPlugin consumers gain correct expiry. Rendering is explicit opt-in. Games keep ownership of camera, drop mappings, and the single collector/primary container. Custom rendering can omit the visual plugin or attach its own Mesh3d before PostUpdate.
