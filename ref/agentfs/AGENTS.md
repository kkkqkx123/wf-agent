# AgentFS Project Context

## Project Overview

AgentFS is a filesystem explicitly designed for AI agents. It provides storage abstractions that AI agents need, similar to how traditional filesystems provide file and directory abstractions for applications. The project is currently in alpha stage and licensed under MIT.

### Core Components

1. **SDK** - Libraries for programmatic filesystem access in TypeScript, Python, and Rust
2. **CLI** - Command-line interface for managing agent filesystems with features like FUSE mounting on Linux and NFS on macOS
3. **Specification** - SQLite-based agent filesystem specification that defines the schema and operations

### Key Features

- **Auditability**: Every file operation, tool call, and state change is recorded in a SQLite database file
- **Reproducibility**: Snapshot an agent's state at any point with `cp agent.db snapshot.db`
- **Portability**: The entire agent runtime (files, state, history) is stored in a single SQLite file
- **Sandboxing**: Copy-on-write filesystem isolation for secure agent execution
- **Multi-platform**: Supports Linux (FUSE + overlay) and macOS (NFS + overlay)

## Architecture

AgentFS implements three main interfaces in a SQLite database:

1. **Filesystem**: POSIX-like filesystem for files and directories
2. **Key-Value**: Key-value store for agent state and context  
3. **Toolcall**: Toolcall audit trail for debugging and analysis

The core technology uses Turso (an in-process SQL database compatible with SQLite) to store everything in a single SQLite database file.

## Building and Running

### Prerequisites
- Rust (latest stable version)
- Cargo package manager

### Building the CLI
```bash
cd cli
cargo build --release
```

### Installing the CLI
```bash
curl -fsSL https://agentfs.ai/install | bash
```

### Common Commands
```bash
# Initialize a new agent filesystem
agentfs init my-agent

# List files in agent filesystem
agentfs fs ls my-agent

# Read a file
agentfs fs cat my-agent hello.txt

# Mount agent filesystem
agentfs mount my-agent ./mnt

# Run in sandboxed environment
agentfs run /bin/bash

# View agent timeline
agentfs timeline my-agent
```

### Development Commands
```bash
# Run tests
cargo test

# Run specific tests
cd cli && cargo test

# Benchmark
cargo bench
```

## Development Conventions

### Code Structure
- `/cli` - Main CLI application implementation in Rust
- `/sdk` - Language-specific SDKs (Rust, TypeScript, Python)
- `/examples` - Integration examples with popular AI frameworks
- `/sandbox` - Sandboxing implementation for secure execution
- `/specs.md` - Technical specification of the agent filesystem

### Testing
The project uses multiple testing approaches:
- Unit tests in Rust with `cargo test`
- POSIX filesystem compliance testing with `pjdfstest`
- Filesystem stress testing with `xfstests`

### SDK Development
The SDK follows a consistent API across languages with three main interfaces:
- Filesystem operations (readFile, writeFile, readdir, etc.)
- Key-value store operations (get, set, delete, list)
- Tool call recording (record, query)

## Project Status

- **Current Version**: 0.6.0-pre.1 (alpha)
- **Status**: Alpha - for development, testing, and experimentation only
- **Platforms**: Linux, macOS, Windows (limited)
- **Architecture Support**: x86_64, ARM64

## Security Features

- Local encryption support using libSQL's encryption (AES-256-GCM, AEGIS variants)
- Sandboxed execution with copy-on-write filesystems
- Isolated agent environments preventing cross-contamination
- Network-transparent operation via NFS and MCP protocols

## Integration Examples

The project includes examples for popular AI frameworks:
- Mastra AI framework
- Anthropic's Claude Agent SDK
- OpenAI Agents SDK
- Vercel AI SDK with just-bash
- Cloudflare Workers with Durable Objects

## Contributing

The project welcomes contributions. See the CONTRIBUTING.md file in the CLI directory for detailed guidelines.