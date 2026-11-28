# Design

## Where the "modified" state lives

A private `dirty: bool` inside `Chunk<V>`, set by `set` / `set_state` when the
stored value differs.

Alternatives rejected:

- **Split `ChunkModified` into "data changed" and "needs remesh".** Mining and
  placement notify border neighbours through the same public `MessageWriter`
  their pure functions receive, so a split changes the signatures of
  `tick_mining`, `try_place_voxel` and `set_voxel_at`, and every downstream
  reader of the message. It also does not answer "should this chunk be saved
  on unload", which needs per-chunk state anyway.
- **A dirty-set resource in `ChunkManager`.** `Chunk::set` cannot reach a
  resource, so every edit path would have to remember to record the coord;
  `set_state_at` and direct `Query<&mut Chunk>` edits would be missed.
- **A revision counter plus a `SavedRevision` component.** Works, but three
  spawn paths (task poll, synchronous restore, hand-spawned chunks) would each
  need to attach the baseline, and a missing baseline needs a policy.

With the flag inside the chunk there is no missing-baseline case: a chunk built
by hand with `set` is dirty and is saved as before; an untouched
`Chunk::new()` is clean.

## Who clears the flag

The producer of the data, not the consumer:

- `try_load_chunk` returns a clean chunk (it matches its file). The low-level
  `load_chunk` is left alone, so a chunk imported from somewhere else stays
  dirty and is saved into the world directory.
- The background task calls `mark_clean()` after `ChunkGenerator::generate`,
  because generators fill chunks through `set`.
- `chunk_autosave_system` clears it after a successful write, through
  `bypass_change_detection()` so clearing a bookkeeping flag does not look like
  a data change to `Changed<Chunk<V>>` queries.

A failed write leaves the chunk dirty, so it is retried on the next
`ChunkModified` or on unload.

## Autosave still reads `ChunkModified`

The message stays the trigger; the dirty flag is the filter. Remesh-only
messages now find a clean chunk and write nothing. No ordering change is
needed: a chunk can only be dirty after something obtained `&mut Chunk`, so
its component exists by then, and the pre-existing "component not attached
yet" miss can only affect clean chunks.

## Restore in the task

The task closure tries `try_load_chunk` and falls back to the generator. The
public `ChunkTask<V>` keeps its `Task<(ChunkCoord, Chunk<V>)>` type, so the
poll system is unchanged and treats restored and generated chunks alike.

That type cannot express "no saved chunk and nothing to generate", so the
no-generator path keeps the synchronous restore. It is a tooling/test
configuration; a game always has a generator.

No race is introduced: all writes are synchronous on the main thread, and
streaming requests a coord only when it is neither loaded nor pending, so the
task for a coord starts after any save of that coord has returned.

## Shared save helper

`save_chunk_to_dir` (`pub(crate)`, `world/persistence.rs`) holds the path,
`create_dir_all` and `save_chunk` sequence used by both autosave and unload.
Unload now also skips the write when the directory cannot be created.

## Known limits

- Direct writes to the public `states` array bypass tracking.
- State-only edits are saved on unload but not at app exit (unchanged).
- Untouched terrain is regenerated, so it follows generator or seed changes.
