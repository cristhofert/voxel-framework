# Design
Use ChunkMaterialConfig.alpha_mode (Bevy AlphaMode), default Opaque, forwarded to the shared StandardMaterial at Startup. Opaque restores depth writes and opaque-pass occlusion. Air remains absent geometry, independent of material alpha mode.
Explicit Blend preserves the old opt-in behavior, including its existing sorting limitations: all chunks share one material and mixed opaque/translucent content is not split.
No plugin ordering, generic bounds, shader, texture-array material, or persistence changes.
Verify actual startup asset creation headlessly, all default solids and air at both LOD steps, and texture-array selection. Native visual acceptance in Astrohabita remains a separate downstream check; automated tests do not claim screenshot evidence.
