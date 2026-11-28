# Tasks

## 1. Chunk state

- [x] 1.1 `dirty` flag in `Chunk<V>`; `is_dirty`, `mark_clean`, `mark_dirty`; `set` / `set_state` mark on change. Tests.

## 2. Saving

- [x] 2.1 `try_load_chunk` returns a clean chunk; `save_chunk_to_dir` helper.
- [x] 2.2 `chunk_autosave_system` skips clean chunks and clears the flag after a successful write. Tests.
- [x] 2.3 `chunk_despawn_system` saves only dirty chunks. Tests.

## 3. Loading

- [x] 3.1 Restore inside the background task; generated chunks marked clean; synchronous path kept without a generator. Unit tests for `generator/systems.rs`.
- [x] 3.2 Integration tests: restore without regenerating or rewriting, damaged file falls back, generated terrain is not written.

## 4. Docs

- [x] 4.1 Rustdoc of the four modules and `ChunkModified`.
- [x] 4.2 `README.md`, `CHANGELOG.md`, `examples/basic_world.md`, `docs/architecture.md`, `docs/multiplayer-readiness.md`.

## 5. Verify

- [x] 5.1 Local checks and review; `verify-report.md`.
