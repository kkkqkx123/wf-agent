# Layertwine

**Layertwine** — A lightweight file-edit history storage layer for multi-agent collaborative editing + human review workflows.

Layertwine is an infra-only library crate: it is embedded in-process (via `wf-checkpoint`) and ships no CLI, HTTP/gRPC server, or standalone binary. It is not published as an independent package.

[中文文档 (Chinese Documentation)](README_zh.md)

## Features

- **Layered State Machine**: Isolated edit layers (`manual_edit`, `agent_edit`, `approval`, `staged`) with controlled transitions
- **Immutable Snapshots**: All edits create immutable snapshots stored as line-level deltas
- **Checkpoint Repository**: Commit history with branching, merging, and DAG-based ancestry
- **Agent Collaboration**: Dedicated agent edit flow with human approval workflow
- **GC**: Redundant-checkpoint garbage collection driven by the embedding crate

## Why Layertwine?

Traditional version control (Git) cannot handle uncommitted changes from multiple sources. Layertwine solves this by:

1. **Tracking uncommitted edits**: Records changes before they reach Git
2. **Source attribution**: Distinguishes manual edits vs. agent-generated changes
3. **Human approval gate**: Agent changes require human review before integration
4. **Safe rollbacks**: Full audit trail enables point-in-time recovery

## Quick Start

Add the crate as a path dependency and use the storage engine directly:

```rust
use layertwine::storage::SqliteStorage;
use std::path::Path;

let storage = SqliteStorage::new_full(Path::new(".layertwine/layertwine.db"))?;
```

In the `wf-agent` workspace, production callers go through `wf-checkpoint`
(`FileCheckpointManager`), which owns attribution, sampling, and merge
policy on top of this engine. There is no standalone service facade:
the former `ApiService`, backup repo, TOML config chain, and Git bridge
were removed when the independent-package direction was abandoned.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│               Checkpoint Repository                          │
│   (branches, commits, DAG history)                           │
└───────────────────────────────┬─────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────┐
│  Layered State Machine                                       │
│  ┌───────────────────┐                                       │
│  │ manual_edit       │                                       │
│  │ agent_edit        │──► approval ──► integrated ──► staged  │
└─────────────────────────────────────────────────────────────┘
```

### Core Concepts

| Concept | Description |
|---------|-------------|
| **Layer** | Edit isolation boundary (manual, agent, approval, staged) |
| **Partition** | Within-layer workspace (e.g., `agent:agent-01`) |
| **Snapshot** | Immutable file-state captured at a point in time |
| **Delta** | Line-level change description (insert/delete/replace) |
| **Checkpoint** | Named commit linking to one or more snapshots |
| **Branch** | Movable pointer to checkpoint lineage |

## Embedding

Layertwine runs in-process. The typical flow (executed by `wf-checkpoint`)
is: open `SqliteStorage`, apply layered edits via `layered::*`, then
persist checkpoints via `CheckpointRepo`.

There are no network transports and no feature flags: the former CLI,
HTTP, and gRPC layers were removed when Layertwine became an infra-only
library crate, along with the standalone `ApiService`, backup repo,
TOML config chain, and Git bridge.

## Multi-Agent Workflow

Each agent edits in its own isolated partition and submits for review;
a human approves, then approvals merge to staged and commit — driven
through `wf-checkpoint` on top of this engine.

## Data Model

### Immutability Guarantees

- **Snapshots**: INSERT-only, never modified or deleted
- **Deltas**: INSERT-only, form immutable chains
- **Checkpoints**: INSERT-only, form DAG through parent references

### Mutable State

Only partition pointers and layer state are mutable:
- `partitions`: Current snapshot reference
- `partition_history`: Delta chain per partition
- `layers`: Transition metadata

### Content-Addressed IDs

Snapshot, Checkpoint and FileNode IDs are Blake3 hashes of their canonical JSON representation. Delta IDs additionally include the edit timestamp and a per-process monotonic invocation counter, so each edit-tool call gets a unique record even when the content change is identical:
```rust
let id = blake3::hash(serde_json::to_vec(&entity).unwrap());
```

## Storage

- **Database**: SQLite (embedded, single-file, transactional)
- **Compression**: Zstd compression for large delta chains
- **Maintenance**: Built-in GC, VACUUM support, WAL checkpointing

## Git Integration

GC is driven by the embedding crate through the storage engine.

## Testing

```bash
# Unit tests
cargo test -p layertwine --lib

# All layertwine tests (unit + integration)
cargo test -p layertwine
```

## Performance Benchmarks

```bash
# Run benchmarks
cargo bench -p layertwine
```

## Error Handling

Layertwine provides structured error types (`LayertwineError`, `StorageError`)
covering storage, engine, checkpoint, integrity, and GC failures.
All errors carry actionable detail strings.

## Project Structure

```
src/
├── core/           # Immutable data types (FileNode, Delta, Snapshot)
├── storage/        # SQLite persistence (SqliteStorage, migrations)
├── engine/         # Diff/merge/word-diff operations
├── layered/        # Layer implementations (manual, agent, approval...)
├── checkpoint/     # Checkpoint repository (branch, repo, types, gc)
└── error.rs        # Error type definitions

tests/
├── engine_integration.rs
├── layered_integration.rs
└── ...
```

## Documentation

- [Architecture Overview](docs/architecture/01-架构总览.md)
