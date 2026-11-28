# Persistence Specification

## Purpose

Define when chunk data is written to and read from disk by
`PersistencePlugin`, the generator pipeline and chunk unloading
(`src/world/persistence.rs`, `src/generator/systems.rs`,
`src/mesh/systems.rs`, `src/chunk/storage.rs`).

Source of truth: this file. Last updated by archive of `chunk-persistence-dirty` on 2026-11-28.

## Requirements

### Requirement: Unsaved-change tracking

A `Chunk<V>` MUST report through `is_dirty()` whether its voxels or state
bytes changed since it was created, generated, restored or last saved. A chunk
from `Chunk::new()` MUST be clean. `set` and `set_state` MUST mark the chunk
dirty when they change a stored value and MUST NOT when they write the value
already stored. `mark_clean()` and `mark_dirty()` MUST set the flag directly.
Direct writes to the public `states` array are not observed; callers MUST call
`mark_dirty()` after them.

#### Scenario: Editing marks the chunk

- GIVEN a clean chunk
- WHEN a voxel or a state byte is set to a different value
- THEN `is_dirty()` MUST return `true`

#### Scenario: Writing the same value

- GIVEN a clean chunk
- WHEN a voxel and a state byte are set to the values they already hold
- THEN `is_dirty()` MUST return `false`

### Requirement: Loaded chunks start clean

Chunks produced by the generator pipeline MUST be clean when they are attached
to their entity, whether they were generated or restored from disk.
`try_load_chunk` MUST return a clean chunk. The low-level `load_chunk` is not
part of this requirement.

#### Scenario: Generated chunk

- GIVEN a generator that fills voxels through `Chunk::set`
- WHEN the pipeline finishes loading a chunk with no saved file
- THEN the chunk MUST be clean

### Requirement: Autosave writes only unsaved changes

With `PersistenceConfig::auto_save` enabled, `chunk_autosave_system` MUST
write a chunk named by a `ChunkModified` message only when the chunk is dirty,
and MUST mark it clean after a successful write. A clean chunk MUST NOT be
written. When the write fails the chunk MUST stay dirty. With `auto_save`
disabled nothing MUST be written.

#### Scenario: Remesh-only notification

- GIVEN a loaded clean chunk
- WHEN `ChunkModified` is sent for it
- THEN no file MUST be written

#### Scenario: Edit then repeat notification

- GIVEN a loaded chunk edited through `set_voxel_at`
- WHEN autosave has written it and `ChunkModified` is sent again without a new edit
- THEN the chunk MUST NOT be written a second time

#### Scenario: Failed write

- GIVEN a dirty chunk and a save directory that cannot be created
- WHEN autosave runs
- THEN the chunk MUST remain dirty

### Requirement: Unload saves only unsaved changes

With `auto_save` enabled, `chunk_despawn_system` MUST save a chunk before
despawning it only when the chunk is dirty. This MUST include changes that
never produced a `ChunkModified` message, such as state-only edits. A clean
chunk MUST be despawned without writing a file.

#### Scenario: Unmodified terrain streams out

- GIVEN a loaded clean chunk
- WHEN it is unloaded
- THEN it MUST be despawned and no file MUST exist for it

#### Scenario: State-only edit streams out

- GIVEN a loaded chunk whose only change is a state byte
- WHEN it is unloaded
- THEN its file MUST contain the new state byte

### Requirement: Restore runs off the main thread

When a `GeneratorResource` and a `PersistenceConfig` exist,
`chunk_request_system` MUST NOT read chunk files itself. The background load
task MUST try the saved file first and MUST run the generator only when no
readable file exists. A saved chunk MUST win over the generator. A file that
cannot be read MUST fall back to the generator.

Without a `GeneratorResource`, a saved chunk MUST be restored and registered
in the same run of `chunk_request_system`, and a request with no saved file
MUST be skipped.

#### Scenario: Saved chunk with a generator

- GIVEN a saved chunk file and a registered generator
- WHEN `chunk_request_system` handles the load request
- THEN the chunk MUST NOT be registered in that run, and once the task
  completes the chunk MUST hold the saved data and the generator MUST NOT have
  been called

#### Scenario: Damaged file

- GIVEN a chunk file shorter than one chunk and a registered generator
- WHEN the chunk is loaded
- THEN the generator MUST produce it

### Requirement: Restoring does not rewrite

A chunk restored from disk MUST NOT be written back to disk until its data
changes.

#### Scenario: Restore then notification

- GIVEN a chunk restored from its file with `auto_save` enabled
- WHEN the `ChunkModified` message that triggers its first mesh is processed
- THEN its file MUST NOT be written
