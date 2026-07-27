# Developer Guide

This document provides essential information for AI agents working with the Modular Agent Framework.

**No-backward-compatible**
At present, the project is in the development stage and there is no need to specifically consider backward compatibility. It is important to maintain a reasonable architecture.

## Language

Always use English in code, comments, logging, error info. Use Chinese in docs.
**Never use any Chinese in any code files.**

## Project Overview

The Modular Agent Framework is a monorepo undergoing **Rust migration** (see `docs/plan/rust迁移-分阶段方案.md`). Currently in hybrid state:

### TypeScript Layer (deprecated, read-only reference)

**⚠️ DO NOT MODIFY ANY FILES UNDER `packages/`**

The TypeScript codebase is fully deprecated and serves **only** as a behavioral reference for Rust migration. Do not fix bugs, refactor, add features, or run `tsc`/`lint` on TS files. Any TS changes are wasted effort.

- **SDK Module**: TypeScript workflow execution engine with 15+ node types
- **Multi-model LLM integration**: OpenAI, Anthropic, Gemini, Mock
- **Flexible tool system**: Built-in, native, REST, MCP
- **Fork/Join support**: Parallel execution capabilities
- **Checkpoint mechanism**: State snapshots and resumption
- **Event-driven architecture**: Extensibility features
- **Shared packages**: Reusable utilities and components
- **Application modules**: Ready-to-deploy applications

### Rust Crate Layer (in progress, P0-P1 completed)

Crates under `crates/` with a strict dependency DAG:

- `wf-types` - All type definitions (serde), 20 node types, workflow/agent/checkpoint types
- `wf-common` - Common utilities (error, result, time, id)
- `wf-storage` - Storage adapter traits + in-memory/SQLite/PostgreSQL implementations

## Code Architecture

### Monorepo Structure

```
wf-agent/
├── apps/               # Application modules (TS)
├── packages/           # Shared TS packages
│   ├── common-utils/   # Common utilities
│   ├── config-processor/
│   ├── sdk/            # Core SDK package
│   ├── sdk-kit/        # SDK toolkit package
│   ├── storage/        # Storage utilities
│   └── types/          # Type definitions (Zod schemas)
├── Cargo.toml           # Workspace definition
├── rust-toolchain.toml  # Rust toolchain config
├── crates/              # Rust crates (migration target)
│   ├── wf-types/        # Type definitions (serde)
│   ├── wf-common/       # Common utilities
│   ├── wf-storage/      # Storage implementations
│   ├── wf-core/         # EventBus, StateMachine, Registry
│   ├── wf-checkpoint/    # Checkpoint system
│   ├── wf-config/        # Configuration processing
│   ├── wf-tools/         # Tool registry, executors, MCP
│   ├── wf-llm/           # LLM client abstraction
│   ├── wf-execution-shared/  # Shared execution infrastructure
│   ├── wf-agent/         # Agent loop execution engine
│   ├── wf-workflow/      # Workflow graph execution engine
│   ├── wf-runtime/       # Runtime bootstrap
│   └── wf-sandbox/       # Script sandbox
├── crates/layertwine/   # Standalone file-edit history (not in workspace)
├── package.json
├── pnpm-workspace.yaml
└── turbo.json
```

### Rust Crate Dependency DAG

```
wf-types  ←  wf-storage  →  wf-common
    ↓           ↓
wf-core ←──────┘
    ↓
wf-tools ←──── wf-execution-shared
    ↓                ↓
wf-llm         wf-agent
                   ↓
              wf-workflow
```

:## Rust Development Conventions

### Module Structure

Each crate uses `include!` instead of `mod` in `lib.rs`. The crate root file is named after the directory (e.g., `wf_types.rs` for `wf-types`). Sub-files use flat includes — no nested module directories.

### File Layout Pattern

```
crates/<name>/src/
├── lib.rs              ← include!("<crate_name>.rs");
└── <crate_name>.rs     ← root, has all imports, includes sub-files
```

## Building and Running

Prerequisites: rustc 1.88.0, cargo 1.88.0

```shell
cargo clippy --all-targets --all-features            # full compile check
cargo check -p graphdb --features server,fulltext-search,c_api,grpc,qdrant  # check with all features
```

## Development Conventions

- Rust standard formatting (`cargo fmt`)
- Modular design following Rust conventions

## Testing

```shell
cargo test --lib -- --nocapture               # lib tests
cargo test --test '*' -- --nocapture           # integration tests
cargo test <test_name>                         # specific test(s)
```

Test organization: unit tests in same file (`#[cfg(test)]`), separate `test.rs` for large files, integration tests in `tests/`, benchmarks in `benches/`.

## Coding Standards

- **Security**: Never use unwrap (use expect in tests). No unsafe except low-level ops, documented in `docs/archive/unsafe.md`.
- **Types**: Minimize `dyn`, prefer concrete types. All dynamic dispatch documented in `docs/archive/dynamic.md`.
- **Dependencies**: All sub-crates form a strict DAG (no circular deps between crates).

## Important Notes

1. **Rust deps**: Centralized in root `Cargo.toml` workspace section
2. **Plan/Design Document**: Avoid including complete code snippets. Mainly using concise natural language descriptions.

## Package Structure Management

The use of `mod.rs` files is prohibited. All modules must be defined as `<module_name>.rs` files in the parent directory (e.g., `foo.rs` instead of `foo/mod.rs`).

