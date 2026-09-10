# Layertwine

[![Crates.io](https://img.shields.io/crates/v/layertwine.svg)](https://crates.io/crates/layertwine)
[![Documentation](https://docs.rs/layertwine/badge.svg)](https://docs.rs/layertwine)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Layertwine** — A lightweight file-edit history storage layer for multi-agent collaborative editing + human review workflows.

Layertwine is an infra-only library crate: it is embedded in-process (via `wf-checkpoint`) and ships no CLI, HTTP/gRPC server, or standalone binary.

[中文文档 (Chinese Documentation)](README_zh.md)

## Features

- **Layered State Machine**: Isolated edit layers (`manual_edit`, `agent_edit`, `approval`, `staged`) with controlled transitions
- **Immutable Snapshots**: All edits create immutable snapshots stored as line-level deltas
- **Checkpoint Repository**: Git-like commit history with branching, merging, and DAG-based ancestry
- **Agent Collaboration**: Dedicated agent edit flow with human approval workflow
- **Git Synchronization**: Bidirectional sync between Layertwine checkpoints and Git commits
- **Snapshot Backup**: Physical isolation backup system for safety-critical restore points
- **In-process API**: A single `ApiService` facade over the storage engine for same-process callers

## Why Layertwine?

Traditional version control (Git) cannot handle uncommitted changes from multiple sources. Layertwine solves this by:

1. **Tracking uncommitted edits**: Records changes before they reach Git
2. **Source attribution**: Distinguishes manual edits vs. agent-generated changes
3. **Human approval gate**: Agent changes require human review before integration
4. **Safe rollbacks**: Full audit trail enables point-in-time recovery

## Quick Start

Add the crate as a dependency and open the in-process service:

```rust
use layertwine::api::{ApiService, CommitRequest, EditRequest, ServiceConfig};

let service = ApiService::open(ServiceConfig {
    db_path: ".layertwine/layertwine.db".into(),
    workspace_key: None,
})?;
```

In the `wf-agent` workspace, production callers go through `wf-checkpoint`
(`FileCheckpointManager`), which owns attribution, sampling, and merge
policy on top of this engine. Use `ApiService` directly only for tests and
tooling.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│               Checkpoint Repository                          │
│   (branches, commits, DAG history)                           │
└───────────────┬───────────────────────────────┬─────────────┘
                │                               │
                ▼                               ▼
┌─────────────────────────┐     ┌──────────────────────────────┐
│  Layered State Machine  │     │     Snapshot Backup          │
│  ┌───────────────────┐  │     │  (physical isolation)        │
│  │ manual_edit       │  │     └──────────────────────────────┘
│  │ agent_edit        │──┼────────► approval ◄── Agent Flow
│  │ staged            │  │
│  └───────────────────┘  │
└─────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Git Repository                            │
│   (long-term persistence, periodic sync)                     │
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
is: open the service, apply layered edits, then persist checkpoints.

```rust
let edit = service.edit(EditRequest {
    file: "src/main.rs".into(),
    content: Some("fn main() {}\n".into()),
})?;
let commit = service.commit(CommitRequest {
    message: "initial commit".into(),
    author: Some("dev-1".into()),
})?;
```

There are no network transports and no feature flags: the former CLI,
HTTP, and gRPC layers were removed when Layertwine became an infra-only
library crate.

## Multi-Agent Workflow

Each agent edits in its own isolated partition and submits for review;
a human approves, then approvals merge to staged and commit — the same
lifecycle as before, now driven through `ApiService` (or `wf-checkpoint`)
instead of shell commands.

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

Git sync (checkpoint export/import) and GC are driven by the embedding
crate through the storage engine. Git sync is opt-in and does not interfere
with active editing workflows.

## Testing

```bash
# Unit tests
cargo test --lib

# All tests (unit + integration + e2e)
cargo test

# E2E tests only
cargo test --test e2e_tests
```

## Performance Benchmarks

See [benches/PERFORMANCE_ANALYSIS.md](benches/PERFORMANCE_ANALYSIS.md) for detailed performance analysis.

```bash
# Run benchmarks
cargo bench
```

## Error Handling

Layertwine provides structured error types (`LayertwineError`, `StorageError`)
covering storage, engine, checkpoint, restore, integrity, git-sync, and GC
failures. All errors carry actionable detail strings.

## Project Structure

```
src/
├── core/           # Immutable data types (FileNode, Delta, Snapshot)
├── storage/        # SQLite persistence (SqliteStorage, migrations)
├── engine/         # Diff/merge/word-diff operations
├── layered/        # Layer implementations (manual, agent, approval...)
├── checkpoint/     # Checkpoint repository (branch, dag, repo)
├── backup/         # Snapshot backup module
├── git_sync/       # Git synchronization & GC
├── api/            # In-process service facade & type definitions
├── config/         # Configuration management
└── error.rs        # Error type definitions

tests/
├── common/         # Test fixtures and helpers
├── e2e/            # End-to-end test scenarios
└── ...
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Write tests alongside code
4. Ensure all tests pass: `cargo test`
5. Submit a pull request

## License

MIT License - see [LICENSE](LICENSE) for details

## Documentation

- [Architecture Overview](docs/architecture/01-架构总览.md)

---

Built with ❤️ using Rust
