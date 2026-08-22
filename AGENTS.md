# Developer Guide

This document provides essential information for AI agents working with the Modular Agent Framework.

**No-backward-compatible**
At present, the project is in the development stage and there is no need to specifically consider backward compatibility. It is important to maintain a reasonable architecture.

**Document Reference Rule**
Prohibit the use of any document structure identifiers (e.g., P1, P2-3, §4.1, phase3, G2, etc.) in code comments. Comments shall describe code intent only and shall not reference external document locations.

## Language

Always use English in code, comments, logging, error info. Use Chinese in docs.
**Never use any Chinese in any code files.**

## Project Overview

A modular agent framework that unifies graph-based workflow orchestration with autonomous LLM agent loops in a Rust.

## Code Architecture

```
wf-agent/
├── apps/               # Application modules (TS)
│   ├── vscode-app/
│   └── web-app/
├── Cargo.toml           # Workspace definition
├── rust-toolchain.toml  # Rust toolchain config
├── crates/              # Rust crates (migration target)
│   ├── wf-types/        # Type definitions (serde)
│   ├── wf-common/       # Common utilities
│   ├── wf-storage/      # Storage implementations
│   ├── wf-core/         # EventBus, StateMachine, Registry
│   ├── wf-checkpoint/   # Checkpoint system
│   ├── wf-config/       # Configuration processing
│   ├── wf-shell/        # Shell/terminal engine (PTY, sessions, detector)
│   ├── wf-tools/        # Tool registry, executors, MCP
│   ├── wf-llm/          # LLM client abstraction
│   ├── wf-plugin/       # Plugin system (Lua/Native)
│   ├── wf-script/       # Script expression evaluation
│   ├── wf-execution-shared/  # Shared execution infrastructure
│   ├── wf-agent/        # Agent loop execution engine
│   ├── wf-workflow/     # Workflow graph execution engine
│   ├── wf-runtime/      # Runtime bootstrap
│   ├── wf-sandbox/      # Script sandbox
│   └── wf-cli/          # CLI: headless run / mini / full TUI forms
├── crates/layertwine/   # File-edit history storage (in workspace)
├── package.json
├── pnpm-workspace.yaml
└── turbo.json
```

### Rust Crate Dependency DAG

```
wf-types  ←  wf-storage  →  wf-common
    ↓           ↓               ↓
wf-core ←──────┘          wf-shell  wf-config  wf-script
    ↓                          ↓
    ├── wf-checkpoint           └── wf-tools   wf-llm   wf-plugin
    └── wf-execution-shared
              ↓
         wf-agent
              ↓
    ├── wf-workflow  ──  wf-sandbox
    └── wf-runtime
```

## Rust Development Conventions

### Module Structure

Each crate's `lib.rs` directly declares `pub mod` for sub-modules and `pub use` for public exports. Sub-files use flat naming — no nested module directories, no `mod.rs`.

### File Layout Pattern

```
crates/<name>/src/
├── lib.rs              ← all pub mod declarations and pub use re-exports
├── <module_name>.rs    ← sub-module implementation
├── <other_module>.rs   ← sub-module implementation
└── ...
```

## Building and Running

Prerequisites: latest stable Rust (see `rust-toolchain.toml`)

```shell
cargo clippy --all-targets --all-features            # full compile check
```

## Development Conventions

- Rust standard formatting (`cargo fmt`)
- Modular design following Rust conventions

## Testing

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
