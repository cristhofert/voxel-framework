# Opaque chunk fallback
## Intent
Fix missing-looking solid terrain in consumers using vertex colors without a texture array. DefaultVoxel already gives every solid alpha 1, but the shared fallback forces Blend, which disables depth writes in Bevy.
## Scope
Default the fallback to Opaque; expose explicit AlphaMode configuration for custom palettes. Preserve air meshing, colors, texture rendering, and unrelated drop work.
## Consumer impact
Fully specified ChunkMaterialConfig literals need an alpha_mode field or ..Default::default(). Consumers requiring the previous blend behavior must explicitly select Blend. This does not add per-voxel material separation or triangle sorting.
## Rollback
Revert the config field and fallback change together; no save format or trait changes.
