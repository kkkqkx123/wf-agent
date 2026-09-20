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

Top level contains `apps/`, `Cargo.toml`, `rust-toolchain.toml`, `crates/`, `package.json`, `pnpm-workspace.yaml`, `turbo.json`.

`apps/` contains `vscode-app`, `web-app`.

`crates/` contains four layers: `foundation/`, `infra/`, `engine/`, `app/`.

foundation layer: `wf-types`, `wf-common`, `wf-core`

infra layer: `wf-metrics`, `wf-config`, `wf-storage`, `wf-llm`, `wf-script`, `wf-sandbox`, `wf-shell`, `wf-plugin` (Lua/Native plugin system), `checkpoint/` (checkpoint subsystem: state + file history policy + storage engine)

engine layer: `wf-tools` (tool registry, executors, MCP), `wf-resource` (resource management: registries, rendering, custom resources), `wf-execution-shared` (shared execution infrastructure), `wf-agent` (agent loop execution engine), `wf-workflow` (workflow graph execution engine)

app layer: `wf-api` (application-facing API facade), `wf-server` (HTTP transport layer), `wf-runtime` (runtime bootstrap), `cli/` (CLI frontends, peer of a future desktop app), `tui/` (low-level TUI building blocks, no `wf-` prefix)

cli layer: `wf-cli-shared` (TUI-free shared logic), `wf-headless` (headless CLI binary), `wf-mini` (lightweight crossterm TUI binary), `wf-tui` (full ratatui TUI facade: library + `wf` binary), `wf-cli-demo` (runnable examples for all CLI forms)

tui layer: `tui-clock` (monotonic clock helper, leaf), `tui-terminal` (terminal backend, capabilities, sigint, stderr guard), `tui-style` (theme, animation, motion), `tui-markdown` (Markdown streaming + render), `tui-core` (reducer, events, pacing, screen data, headless summary kernel), `tui-components` (widgets / overlays), `tui-render` (frame render + screen draw), `tui-debug` (diff recorder / debug views)

### Rust Crate Dependency DAG

foundation: wf-types ← wf-common ← wf-core

infra: wf-metrics wf-storage wf-config wf-script wf-llm wf-sandbox wf-shell wf-plugin checkpoint/

checkpoint: checkpoint-base (leaf: errors, actors, policies, deltas); checkpoint-state (→ checkpoint-base: execution snapshots, restore); checkpoint-file (→ checkpoint-base: file history, observe, branches); wf-checkpoint (facade → checkpoint-base/checkpoint-state/checkpoint-file: coordinators + re-exports); layertwine (leaf storage engine, used only by checkpoint-file / wf-checkpoint)

engine: wf-tools ← wf-resource ← wf-execution-shared ← wf-agent ← wf-workflow

app: wf-api wf-server wf-runtime

cli: wf-cli-shared ← wf-headless / wf-mini / wf-tui ← wf-cli-demo

tui: tui-clock (leaf); tui-terminal (→ wf-cli-shared); tui-style (→ tui-clock); tui-markdown (→ tui-style); tui-core (→ tui-clock/tui-style/tui-markdown/tui-terminal/wf-api); tui-components (→ tui-core/tui-style/tui-markdown); tui-render (→ tui-components/tui-core/tui-style); tui-debug (→ tui-core)

`wf-tui` is a thin facade that re-exports every `tui/*` crate and keeps the application shell (`tui/interactive/state/screens/fetch/replay/size`). `wf-mini` may depend directly on low-level `tui/*` crates (e.g. `tui-terminal`, `tui-clock`) without the `wf-` prefix.

`app/cli/` groups every CLI frontend under one submodule so a future desktop app can sit as a peer (`app/desktop/`) instead of mixing frontend crates with the core app facade (`wf-api` / `wf-server` / `wf-runtime`).

`app/tui/` holds the low-level TUI building blocks split out of the former monolithic `wf-tui` crate. These crates carry no `wf-` prefix so both `wf-tui` and `wf-mini` can depend on them directly without implying they are top-level application products. The `tui/*` crates form their own strict DAG (leaf: `tui-clock`; `tui-terminal` may depend on `wf-cli-shared` for the shared `CliResult`/`CliError` types, which does not create a cycle because `wf-cli-shared` has no TUI dependencies).

`infra/checkpoint/` groups the checkpoint subsystem split out of the former monolithic `wf-checkpoint` crate, mirroring `app/cli/` + `app/tui/`. Internal crates carry no `wf-` prefix (`checkpoint-base`, `checkpoint-state`, `checkpoint-file`, `layertwine`); only the externally integrated facade keeps the `wf-` prefix (`wf-checkpoint`). Upper layers (`wf-tools` / `wf-agent` / `wf-workflow` / `wf-api` / `wf-runtime`) depend only on the `wf-checkpoint` facade, never on the internal crates directly. The `checkpoint/*` crates form their own strict DAG (leaf: `checkpoint-base` + `layertwine`; `checkpoint-state` → `checkpoint-base`; `checkpoint-file` → `checkpoint-base`; `wf-checkpoint` → `checkpoint-base` / `checkpoint-state` / `checkpoint-file`).

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
