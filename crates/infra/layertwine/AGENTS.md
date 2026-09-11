# Layertwine

## Language

Always use English in code, comments, logging, error info or other string literal. Use Chinese in docs (except code block)
**Never use any Chinese in any code files or code block.**

## Project

`layertwine` is a lightweight file-edit history storage layer for multi-agent + human collaborative editing. Infra-only Rust library crate: embedded in-process via `wf-checkpoint`, no binary, no network transports. Not published as an independent package: the former `ApiService`, backup repo, TOML config chain, and Git bridge were removed.

## Build

```sh
cargo test -p layertwine --lib  # run all unit tests (with compile check)
cargo test -p layertwine         # run all tests (unit + integration)
```

## Architecture

```
src/
├── core/        # immutable data types — FileNode, Delta, Snapshot, Partition, Layer, types
├── storage/     # SQLite persistence — SqliteStorage, migrations, Repository traits
├── engine/      # diff/merge/word-diff — similar-based, three-way merge with conflict detection
├── layered/     # layer ops (manual/agent/approval/staged/integrated/transition) + minimal StateMachine handle
├── checkpoint/  # branch/repo/types/gc
├── lib.rs       # re-exports all modules + pub use error::{LayertwineError, StorageError, StorageResult}
└── error.rs     # LayertwineError + StorageError (thiserror)
```

## Tests

```
tests/
├── engine_integration.rs
├── engine_test.rs
├── layered_integration.rs
└── storage_integration.rs
```

**Unit tests:** `#[cfg(test)] mod tests` blocks inside `src/` (core, engine, storage, layered, checkpoint).
**Shared test helpers in src/:** `src/test_utils.rs` provides `setup_storage()`, `setup_storage_full()`, `create_initial_snapshot()` for `#[cfg(test)]` modules.

## Key patterns

- **Content-addressed IDs:** Blake3 hash of `serde_json::to_vec(self)` for Snapshots, Deltas, Checkpoints
- **Storage:** `Repository` trait = `SnapshotStore + DeltaStore + PartitionStore + FileNodeStore`. Implemented by `SqliteStorage`.
- **Immutable entities are INSERT ONLY:** `file_nodes`, `deltas`, `snapshots` — no UPDATE/DELETE. Mutable state lives in `partitions`, `partition_history`, `layers`.
- **Layers:** `ManualEdit`, `AgentEdit`, `Approval`, `Staged` — partition types mirror these with Agent/Approval being per-Agent-instance-subdivided.
- **Engine diffs:** Uses `similar` crate. `apply_deltas()` applies Delta chain to reconstruct file content. `merge_texts()` does three-way merge with `MergeConflict` result.
- **Inverse deltas** need `old_content` to reconstruct deleted lines (Delete ops don't carry deleted content).
