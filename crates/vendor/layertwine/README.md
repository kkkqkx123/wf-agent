# Layertwine

[![Crates.io](https://img.shields.io/crates/v/layertwine.svg)](https://crates.io/crates/layertwine)
[![Documentation](https://docs.rs/layertwine/badge.svg)](https://docs.rs/layertwine)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Layertwine** — A lightweight file-edit history storage layer for multi-agent collaborative editing + human review workflows.

[中文文档 (Chinese Documentation)](docs/user-guide/01-CLI 使用指南.md)

## Features

- **Layered State Machine**: Isolated edit layers (`manual_edit`, `agent_edit`, `approval`, `staged`) with controlled transitions
- **Immutable Snapshots**: All edits create immutable snapshots stored as line-level deltas
- **Checkpoint Repository**: Git-like commit history with branching, merging, and DAG-based ancestry
- **Agent Collaboration**: Dedicated agent edit flow with human approval workflow
- **Git Synchronization**: Bidirectional sync between Layertwine checkpoints and Git commits
- **Snapshot Backup**: Physical isolation backup system for safety-critical restore points
- **Multi-Transport APIs**: CLI, HTTP REST, and gRPC interfaces sharing the same core logic

## Why Layertwine?

Traditional version control (Git) cannot handle uncommitted changes from multiple sources. Layertwine solves this by:

1. **Tracking uncommitted edits**: Records changes before they reach Git
2. **Source attribution**: Distinguishes manual edits vs. agent-generated changes
3. **Human approval gate**: Agent changes require human review before integration
4. **Safe rollbacks**: Full audit trail enables point-in-time recovery

## Quick Start

### Installation

```bash
# Build with CLI support (default)
cargo install layertwine

# Or use as a library
cargo add layertwine
```

### Initialize a Repository

```bash
# From current directory
layertwine init

# Or from an existing Git repository
layertwine --git-repo /path/to/repo init --git-ref HEAD
```

### Edit and Commit

```bash
# Manual edit
layertwine edit src/main.rs -c "fn main() { println!(\"Hello\"); }"

# Submit checkpoint
layertwine commit -m "Initial commit" -a "developer"

# View history
layertwine log
```

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

## Transport Layers

### CLI

```bash
# All commands support --json output mode
layertwine --help
layertwine status
layertwine branch list
layertwine checkpoint rollback <ID>
```

### HTTP API

```bash
# Start server
LAYERTWINE_MODE=http cargo run --features http

# Initialize
curl -X POST http://127.0.0.1:8080/api/v1/init \
  -H 'Content-Type: application/json' -d '{}'

# Edit file
curl -X POST http://127.0.0.1:8080/api/v1/edit \
  -H 'Content-Type: application/json' \
  -d '{"file":"src/main.rs","content":"fn main() {}"}'
```

### gRPC API

```protobuf
// Connect to localhost:50051
rpc Edit(EditRequest) returns (EditResponse);
rpc Commit(CommitRequest) returns (CommitResponse);
rpc Log(LogRequest) returns (LogResponse);
// ... and 22 more RPC methods
```

## Multi-Agent Workflow Example

```bash
# Agent A makes changes and submits for review
layertwine agent agent-a edit src/auth.rs -c "pub fn login() {}"
layertwine agent agent-a submit

# Agent B makes changes and submits for review
layertwine agent agent-b edit src/db.rs -c "pub fn connect() {}"
layertwine agent agent-b submit

# Review pending submissions
layertwine approval list

# Approve both agents
layertwine approval approve agent-a
layertwine approval approve agent-b

# Merge approvals and commit
layertwine approval merge-to-unified
layertwine approval merge-to-staged
layertwine commit -m "Merge auth and db modules"
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `cli` | Command-line interface (default) |
| `http` | HTTP REST API via Axum |
| `grpc` | gRPC API via Tonic |
| `cli-http` | Combined CLI + HTTP |
| `cli-grpc` | Combined CLI + gRPC |
| `all` | All transport layers |

```bash
# Build with specific features
cargo build --features http,grpc
```

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

All entity IDs are Blake3 hashes of their canonical JSON representation:
```rust
let id = blake3::hash(serde_json::to_vec(&entity).unwrap());
```

## Storage

- **Database**: SQLite (embedded, single-file, transactional)
- **Compression**: Zstd compression for large delta chains
- **Maintenance**: Built-in GC, VACUUM support, WAL checkpointing

## Git Integration

```bash
# Commit Layertwine checkpoints to local Git branch
layertwine --git-repo /path/to/repo git-commit -m "Sync checkpoints"

# Pull remote Git commits into Layertwine
layertwine --git-repo /path/to/repo pull --remote origin --git-ref main
```

Note: Git sync is opt-in and does not interfere with active editing workflows.

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

Layertwine provides structured error types with exit codes:

| Exit Code | Meaning |
|-----------|---------|
| 0 | Success |
| 1 | General error (not found, internal, storage) |
| 2 | Usage error (invalid params, missing arguments) |

All errors include actionable suggestions for resolution.

## Project Structure

```
src/
├── core/           # Immutable data types (FileNode, Delta, Snapshot)
├── storage/        # SQLite persistence (SqliteStorage, migrations)
├── engine/         # Diff/merge/inverse operations
├── state_machine/  # Layer transition logic
├── layered/        # Layer implementations (manual, agent, approval...)
├── checkpoint/     # Checkpoint repository (branch, dag, repo)
├── backup/         # Snapshot backup module
├── git_sync/       # Git synchronization & GC
├── api/            # Shared API service & type definitions
├── cli/            # CLI transport (clap-based)
├── config/         # Configuration management
├── runtime/        # Runtime utilities
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

- [CLI Guide](docs/user-guide/01-CLI 使用指南.md)
- [HTTP API Guide](docs/user-guide/02-HTTP-API 使用指南.md)
- [gRPC API Reference](docs/user-guide/03-gRPC-API 参考.md)
- [Architecture Overview](docs/architecture/01-架构总览.md)

---

Built with ❤️ using Rust
