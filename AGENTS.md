# Developer Guide

This document provides essential information for AI agents working with the Modular Agent Framework.

## Project Overview

The Modular Agent Framework is a monorepo containing:

- **SDK Module**: TypeScript workflow execution engine with 15 node types
- **Multi-model LLM integration**: OpenAI, Anthropic, Gemini, Mock
- **Flexible tool system**: Built-in, native, REST, MCP
- **Fork/Join support**: Parallel execution capabilities
- **Checkpoint mechanism**: State snapshots and resumption
- **Event-driven architecture**: Extensibility features
- **Shared packages**: Reusable utilities and components
- **Application modules**: Ready-to-deploy applications

## Development Environment Setup

### Prerequisites

Node.js v22.0.0+, pnpm v10.28.2, Turbo v2.8.3

### Install Dependencies

```bash
pnpm install
```

### Build

**build first when there are module cross-module issues**

```bash
pnpm build
```

### Testing

```bash
pnpm test
# Run specific package test
pnpm --filter <package-name> test
```

## Code Architecture

### Monorepo Structure

**Apps Layer** (`apps/`)

- Contains application modules
- Uses packages and SDK as dependencies
- Deployable applications

**Packages Layer** (`packages/`)

- Shared utility packages
- Reusable components and libraries
- Cross-project functionality

**SDK Layer** (`sdk/`)

- Core workflow execution engine
- LLM integrations and tool systems
- Detailed architecture in SDK documentation

### Directory Structure

```
modular-agent-framework/
├── apps/  # Application modules
│   ├── web-app/  # Web application
│   └── ...
├── packages/  # Shared packages
│   ├── common-utils/  # Common utilities
│   ├── types/  # Type Definition
│   ├── prompt-templates/  # contain all basic prompt definition
│   └── ...
├── sdk/  # Core SDK module
│   ├── core/  # Core execution logic(shared by agent and graph)
│   ├── api/  # External API interfaces
│   ├── agent/  # Agent-loop implemention
│   ├── graph/  # Graph-workflow implemention
│   └── utils/  # Utility functions
├── package.json  # Root workspace config
├── pnpm-workspace.yaml  # Workspace definitions
└── turbo.json  # Build orchestration
```

## Dependency Management

### Centralized Dependencies

- Common devDependencies defined in root `package.json`
- Shared TypeScript, Vitest, ESLint configurations
- Consistent versions across all packages

### Workspace Protocol

- Use `workspace:*` for internal package references
- Automatic linking between packages
- Version consistency guaranteed

## Development Process

### 1. Package Development

- Create new packages in `packages/` directory
- Add to workspace in parent `package.json`
- Use `workspace:*` for internal dependencies

### 2. App Development

- Create new apps in `apps/` directory
- Reference packages and SDK using `workspace:*`
- Follow consistent build and test patterns

### 3. Testing Strategy

- **Unit tests**: In `__tests__` folders per package
- **Integration tests**: Across package boundaries
- **End-to-end tests**: In `apps/` for complete workflows
- **Run Tests**: `cd <module[like sdk]>; pnpm test <relevant-path of test file/folder>`. **Never run all tests at once. pnpm test without files path is not allowed**
  For example, to run all tests in the `sdk/core/services` package, run `cd sdk; pnpm test core/services`.

### 4. Build Orchestration

- Use Turbo for efficient task execution
- Leverage caching and incremental builds
- Dependency-aware task ordering

## Design Principles

### 1. Modularity

- Clear separation between apps, packages, and SDK
- Independent deployability of components
- Loose coupling with explicit contracts

### 2. Consistency

- Uniform tooling across all packages
- Shared linting and formatting rules
- Standardized project templates

### 3. Scalability

- Efficient dependency management
- Fast builds with caching
- Parallel task execution

## Important Notes

1. **Version Management**: Use root `package.json` for common dependencies
2. **Workspace Protocol**: Always use `workspace:*` for internal references
3. **Build Efficiency**: Leverage Turbo for optimal build performance

## Language

Always use English in code, comments, logging, error info or other string literal. Use Chinese in docs (except code block)
**Never use any Chinese in any code files or code block.**
