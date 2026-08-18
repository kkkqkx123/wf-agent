# Layertwine

## Language

Always use English in code, comments, logging, error info or other string literal. Use Chinese in docs (except code block)
**Never use any Chinese in any code files or code block.**

## Project

`layertwine` is a lightweight file-edit history storage layer for multi-agent + human collaborative editing. Rust library crate (no binary yet).

## Build

```sh
cargo test --lib    # run all unit tests (with compile check)
cargo test          # run all tests (unit + integration + e2e)
cargo test --test e2e_tests  # run only e2e tests
```

## Architecture

```
src/
├── core/        # immutable data types — FileNode, Delta, Snapshot, Partition, Layer, types
├── storage/     # SQLite persistence — SqliteStorage, migrations, Repository traits
├── engine/      # diff/merge/inverse — similar-based, three-way merge with conflict detection
├── state_machine/
├── backup/
├── checkpoint/
├── git_sync/
├── cli/
├── lib.rs       # re-exports all modules + pub use error::{LayertwineError, StorageError, StorageResult}
└── error.rs     # LayertwineError + StorageError (thiserror)
```

## Tests

```
tests/
├── common/                         # Shared test infra: fixture, helpers, assertions, output
│   ├── mod.rs
│   ├── fixture.rs                  # TestConfig, TestEnvironment, TestFixture
│   ├── helpers.rs                  # Convenience wrappers (init, edit, commit, approve, etc.)
│   ├── assertions.rs               # Custom assertions for snapshots, partitions, logs
│   └── output.rs                   # Formatted output utilities
├── e2e/
│   ├── mod.rs                      # Declares all 10 e2e sub-modules
│   └── ...
└── ...
```

**Unit tests:** `#[cfg(test)] mod tests` blocks inside `src/` (core, engine, storage, layered, checkpoint, backup, git_sync, api, cli).
**Shared test helpers in src/:** `src/test_utils.rs` provides `setup_storage()`, `setup_storage_full()`, `create_initial_snapshot()` for `#[cfg(test)]` modules.

## Key patterns

- **Content-addressed IDs:** Blake3 hash of `serde_json::to_vec(self)` for Snapshots, Deltas, Checkpoints
- **Storage:** `Repository` trait = `SnapshotStore + DeltaStore + PartitionStore + FileNodeStore`. Implemented by `SqliteStorage` and `LayertwineStorage` (thin wrapper).
- **Immutable entities are INSERT ONLY:** `file_nodes`, `deltas`, `snapshots` — no UPDATE/DELETE. Mutable state lives in `partitions`, `partition_history`, `layers`.
- **Layers:** `ManualEdit`, `AgentEdit`, `Approval`, `Staged` — partition types mirror these with Agent/Approval being per-Agent-instance-subdivided.
- **Engine diffs:** Uses `similar` crate. `apply_deltas()` applies Delta chain to reconstruct file content. `merge_texts()` does three-way merge with `MergeConflict` result.
- **Inverse deltas** need `old_content` to reconstruct deleted lines (Delete ops don't carry deleted content).
