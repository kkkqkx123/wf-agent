# wf-agent — Modular Agent Framework

A modular Rust framework that unifies graph-based workflow orchestration with autonomous LLM agent loops.

[中文文档 (Chinese Documentation)](README_zh.md)

`wf-agent` runs both execution models on a single shared runtime. Teams can describe deterministic, reviewable pipelines as workflow graphs, run open-ended reasoning as conversational agent loops, and reuse the same tools, checkpoints, sandboxing, and storage across both.

---

## Overview

The framework is organized around two peer execution engines:

- **Workflow engine (`wf-workflow`)** — executes static directed acyclic graphs (DAGs) of typed nodes. Execution routing is defined up front, which makes pipelines inspectable, reproducible, and easy to validate.
- **Agent engine (`wf-agent`)** — executes conversational LLM agent loops. Each iteration calls the model, processes the response (a final answer or tool calls), and repeats until a termination condition is met. The call graph emerges from model decisions at runtime.

Both engines are built on shared execution infrastructure (`wf-execution-shared`) that provides events, hooks, checkpoints, interruption, retries, timeouts, execution hierarchy, and variable scoping. Applications consume the engines through a facade (`wf-api`), an HTTP transport layer (`wf-server`), CLI/TUI frontends, and web or IDE clients.

---

## Core Concepts

### Workflow Execution

A workflow is a graph of nodes connected by edges. Nodes carry a type (`START`, `END`, `LLM`, `CUSTOM`, and more) plus configuration; edges route control flow and map variables between nodes. Before execution, static definitions are compiled into a runtime graph and validated for structure, reachability, and cycles.

The runtime applies:

- Per-node and workflow-level timeouts
- Retry policies with configurable backoff
- Failure policies (`retry`, `continue`, `fail`)
- Checkpointing at configurable boundaries
- Pause, resume, and cancel through interruption handling

Workflow definitions are stored as TOML and support variables, metadata, tags, versioning, and export/import.

### Agent Loop Execution

An agent loop is conversational and iterative. The engine seeds a message history, calls the configured LLM, and lets the model choose between producing a final answer and calling tools. Tool results are appended to the conversation and the loop continues.

Configurable behavior includes:

- Maximum iterations, execution time budget, and pause duration
- Retries, per-call timeouts, and token limits with warning thresholds
- Streaming responses and tool-call protocols (native or text-based)
- Available, initially loaded, discoverable, and hidden tools
- Checkpoint cadence (interval, on error, on tool call) and content selection
- Hooks triggered on iteration, LLM-call, and tool-call boundaries
- Violation policies and dynamic context injection

### Shared Execution Model

Both engines follow a coordinator–executor–handler pattern:

```
Engine lifecycle coordinator
  └─ Executor (stateless, single execution entry)
       └─ Execution coordinator (main loop)
            └─ Per-unit coordinator (per node / per iteration)
                 ├─ Hooks (before / after boundaries)
                 ├─ LLM execution coordinator (shared)
                 ├─ Tool execution coordinator (shared)
                 └─ Goal check and next-step routing
```

This structure keeps lifecycle management, orchestration, and single-purpose handlers separate, and lets both engines address the same shared services.

---

## Features

- **Dual execution engines** — deterministic DAG workflows and autonomous agent loops sharing one runtime.
- **Checkpoint and restore** — snapshot execution state and file history, then restore or resume from any recorded point.
- **Layertwine file history** — a layered, source-attributed edit history for multi-agent collaborative editing with a human approval gate, tracking uncommitted changes that version control alone does not capture.
- **Plugin system** — a unified plugin trait, manifest, and contribution registry with three execution backends: WebAssembly (strong isolation, fuel metering, epoch interruption, WASI authorization), Lua (in-process, bounded standard library), and Native dynamic libraries (fully trusted first-party extensions).
- **Tool registry and MCP** — a registry of built-in and custom tools (filesystem, shell, patch, search, skills) plus Model Context Protocol client support over child-process and streamable HTTP transports.
- **Skill system** — discoverable, reusable capability packages with enable/disable state, content retrieval, resources, and prompt/query endpoints.
- **Script engine** — a resolver and execution engine for language scripts, with risk classification, templates, and flow control.
- **Sandboxing** — policy/strategy/runtime layering for shell, Python, JavaScript, and Lua, combining static analysis gates with real enforcement such as seccomp BPF filters, wrapped runtimes, and an embedded Lua VM. Overlay VFS provides copy-on-write path policy.
- **LLM layer** — provider-agnostic clients with OpenAI, Anthropic, and Gemini wire protocols, OpenAI-compatible provider definitions, profiles, token counting, streaming generation, and provider contribution through plugins.
- **Storage** — entity persistence over SQLite, PostgreSQL, or an in-memory backend, with JSON metadata, filtered queries, bulk batches, cross-entity atomic writes, caching, and instrumentation.
- **Triggers, hooks, and events** — event-driven coordination for scheduling, lifecycle extension, and observability.
- **Human-in-the-loop approval** — tool approval and user interaction records with configurable policies and timeouts.
- **Observability** — metrics collection and export (including Prometheus), execution audit trails, error root-cause analysis, performance profiling, and cross-resource search.
- **Multiple frontends** — headless CLI, lightweight TUI, full ratatui TUI, an HTTP/SSE/WebSocket server, a SvelteKit web app, and a VS Code extension.

---

## Architecture

### Layered Crates

The workspace is organized into four layers. Each layer depends only on layers below it, and all crates form a strict DAG.

| Layer | Crates | Responsibility |
|-------|--------|----------------|
| **Foundation** | `wf-types`, `wf-common`, `wf-core`, `wf-plugin-sdk` | Shared data types, common utilities, core contracts (events, state, hierarchy, interruption, conditions), plugin SDK contracts |
| **Infra** | `wf-metrics`, `wf-config`, `wf-storage`, `wf-llm`, `wf-script`, `wf-sandbox`, `wf-shell`, `wf-plugin` | Cross-cutting services: metrics, configuration, persistence, LLM clients, script engine, sandbox, shell/PTY, plugin runtime |
| **Checkpoint** | `checkpoint-base`, `checkpoint-state`, `checkpoint-file`, `wf-checkpoint`, `layertwine` | Execution snapshots, file-edit history, storage engine, and the integrated facade |
| **Engine** | `wf-tools`, `wf-resource`, `wf-execution-shared`, `wf-agent`, `wf-workflow` | Tool execution and MCP, resource registries and rendering, shared execution infrastructure, agent loop engine, workflow engine |
| **App** | `wf-api`, `wf-server`, `wf-runtime`, `debugger`, CLI crates, TUI crates | Application facade, HTTP transport, runtime bootstrap, debugging, and user interfaces |

The app layer groups frontends under `cli/` and low-level terminal building blocks under `tui/`, allowing the full TUI (`wf-tui`) and the lightweight TUI (`wf-mini`) to reuse the same components.

### Dependency Graph

```
foundation:  wf-types ← wf-common ← wf-core
                                        wf-plugin-sdk

infra:       wf-metrics  wf-config  wf-storage  wf-llm
             wf-script   wf-sandbox wf-shell    wf-plugin
             checkpoint: checkpoint-base ← checkpoint-state
                         checkpoint-base ← checkpoint-file
                         wf-checkpoint   → checkpoint-base / checkpoint-state / checkpoint-file
                         layertwine      (storage engine)

engine:      wf-tools ← wf-resource ← wf-execution-shared ← wf-agent ← wf-workflow

app:         wf-api  wf-server  wf-runtime

cli:         wf-cli-shared ← wf-headless / wf-mini / wf-tui ← wf-cli-demo
tui:         tui-clock ← tui-style ← tui-markdown
             tui-terminal → wf-cli-shared
             tui-core → tui-clock / tui-style / tui-markdown / tui-terminal / wf-api
             tui-components → tui-core / tui-style / tui-markdown
             tui-render → tui-components / tui-core / tui-style
             tui-debug → tui-core
```

### Checkpoint Subsystem

The checkpoint subsystem separates three concerns:

- **`checkpoint-base`** — errors, actors, policies, and deltas.
- **`checkpoint-state`** — execution snapshots and restoration.
- **`checkpoint-file`** — file history, observation, and branches.
- **`layertwine`** — an embedded, content-addressed storage engine built on SQLite with zstd compression, snapshots, deltas, checkpoints, and branching.

`wf-checkpoint` is the only facade exposed to upper layers and owns attribution, sampling, merge policy, and garbage collection. Only mutable state is serialized; immutable configuration is re-supplied on restore.

### Plugin System

`wf-plugin` presents one plugin trait, one manifest format, and one contribution namespace (node types, tool types, LLM codecs, event handlers, middleware) regardless of backend. Which backend executes a plugin stays invisible to the engine.

Isolation strength is tiered by backend:

| Backend | Isolation | Intended use |
|---------|-----------|--------------|
| WebAssembly | Strong — fuel budget, epoch interruption, memory and module-size caps, WASI grants denied by default | Untrusted third-party extensions |
| Lua | Bounded — restricted standard library, interpreter hooks for time and memory, in-process | Trusted lightweight scripts |
| Native | None — dynamic libraries loaded in the host process | Fully trusted first-party extensions |

Wasm supports both core modules and the component model, and can be authored in Rust, Go, or Python.

### Sandbox

The sandbox follows a Policy → Strategy chain → Runtime layering. Analysis strategies act as gates that run before any execution, and execution strategies perform the actual run. Each language ships a default chain:

| Language | Default chain | Enforcement |
|----------|---------------|-------------|
| Shell | static analyzer, VFS gate, OS hook | Command and path analysis plus seccomp BPF and rlimits |
| Python | AST analyzer, builtin hook | AST gate plus import/open/eval restrictions in generated wrapper code |
| JavaScript | VM context | Wrapped execution with proxied modules and disabled dynamic evaluation |
| Lua | static analyzer, embedded VM | Token-level analysis plus API-level isolation in an embedded VM |

Unknown strategies and missing gates fail closed. Strict mode rejects violations, while lenient mode records them and continues. Every decision produces an audit event.

### Application Surfaces

- **`wf-api`** — the application-facing facade. It holds the composition root (`ApiContext`) with engine handles, storage, event bus, metrics, LLM gateway, tool registry, sandbox, and live registries, and exposes functional APIs used by all frontends.
- **`wf-server`** — a pure HTTP transport layer (axum) that maps requests onto `wf-api` calls. It provides a versioned REST surface, Server-Sent Events for execution and event streams, a WebSocket subscription endpoint, a Prometheus metrics endpoint, and middleware for logging, CORS, API-key authentication, and rate limiting.
- **`wf-runtime`** — bootstraps the runtime: registries, storage, LLM, sandbox, and MCP wiring.
- **CLI and TUI** — `wf-headless` for scripts and automation, `wf-mini` for a lightweight terminal interface, and `wf-tui` for the full ratatui interface. Execution tracking is unified across workflow and agent executions and supports blocking, foreground, and background modes.
- **Web app** — a SvelteKit frontend for workflow management, execution monitoring, agent interaction, resource management, and event streaming.
- **VS Code extension** — contributes dynamic editor context (active editor, open tabs, diagnostics) to agent execution.

---

## Repository Layout

```
.
├── crates/
│   ├── foundation/          # types, common utilities, core contracts, plugin SDK
│   ├── infra/               # metrics, config, storage, llm, script, sandbox, shell, plugins
│   │   └── checkpoint/      # checkpoint subsystem (state, file history, storage engine)
│   ├── engine/              # tools, resources, shared execution, agent, workflow
│   └── app/
│       ├── wf-api/          # application facade
│       ├── wf-server/       # HTTP transport layer
│       ├── wf-runtime/      # runtime bootstrap
│       ├── debugger/        # execution debugging
│       ├── cli/             # headless, mini, full TUI, shared logic, demos
│       └── tui/             # low-level terminal building blocks
├── apps/
│   ├── web-app/             # SvelteKit web frontend
│   └── vscode-app/          # VS Code extension
├── configs/                 # workflows, agent loops, templates, LLM profiles, MCP, skills, server
├── docs/                    # architecture, API, CLI, reference, and design documents
└── .wf/                     # project-level configuration overrides
```

---

## Configuration

Configuration is declarative and layered. Project-level files under `.wf/` and the `configs/` directory cover:

- **Workflows** — graph definitions with nodes, edges, variables, retry and checkpoint policy.
- **Agent loops** — model profiles, prompts, tool exposure, checkpoint cadence, hooks.
- **Node and prompt templates** — reusable building blocks.
- **LLM providers and profiles** — provider wire protocols and per-model profiles.
- **MCP** — server connection presets.
- **Skills** — capability package locations.
- **Infrastructure** — sandbox, storage, limits, timeouts, metrics, output, and tool-approval policy.
- **Server** — bind address, authentication, CORS, and rate limits.

Resolution priority runs from CLI flags and SDK options, through project-level `.wf/` files, to global defaults and finally hardcoded processor defaults.

---

## Documentation

Detailed design and architecture references live under `docs/`:

- `docs/architecture/` — agent and workflow engine architecture, plus shared infrastructure.
- `docs/api/` — facade and HTTP transport layer analysis.
- `docs/cli/` — CLI feature set, layout, and component design.
- `docs/infra/` — LLM clients and sandbox architecture.
- `docs/foundation/` — types and plugin author guides.
- `docs/plan/`, `docs/ref/`, `docs/spec/` — planning, reference, and specification material.
