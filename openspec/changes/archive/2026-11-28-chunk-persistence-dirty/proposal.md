# Save only modified chunks, restore off the main thread

A downstream game (on 0.10.4) keeps chunk persistence disabled:
one play session wrote 10,113 chunk files (80 MiB) and loading and flight felt
slower with a saved world. Three causes in the framework:

1. `chunk_despawn_system` saves every unloaded chunk, edited or not.
2. `chunk_request_system` reads saved chunks synchronously in `Update`, and
   probes the disk for every requested chunk.
3. `ChunkModified` is sent both for edits and for remesh-only reasons (first
   mesh after load, neighbour borders, LOD changes), and
   `chunk_autosave_system` writes on all of them. A restored chunk is written
   straight back; generated chunks are written shortly after loading.

Scope:

- `Chunk<V>` tracks unsaved changes: `is_dirty`, `mark_clean`, `mark_dirty`.
- `chunk_autosave_system` and `chunk_despawn_system` save only dirty chunks.
- The generator's background task restores saved chunks; generated and
  restored chunks start clean.
- Out of scope: the chunk file format, compaction or region files, a
  per-frame budget for loading, flushing dirty chunks at app exit, making
  `Chunk::states` private.

Consumer impact: no signature of a public system or function changes and no
public field is added or removed. Behavior changes:

- Untouched terrain is no longer written to disk. If a game changes its
  generator or seed, unedited terrain regenerates with the new output next to
  the edited chunks that were saved.
- A restored chunk becomes available one or more frames after its
  `ChunkLoadRequest` (it used to be registered in the same frame) when a
  `GeneratorResource` exists.
- Direct writes to the public `Chunk::states` array are not tracked and must
  be followed by `Chunk::mark_dirty()`. `Chunk::set_state` is tracked.

Affected modules: `src/chunk/storage.rs`, `src/world/persistence.rs`,
`src/mesh/systems.rs`, `src/generator/systems.rs`.

Rollback: revert the change. The on-disk format is untouched, so save
directories written before or after remain readable either way; files for
untouched terrain written by older versions are still restored.

This is a substantial change (persistence behavior, new public methods). SDD
files are maintained directly because the referenced SDD skills are not
installed in this environment.
