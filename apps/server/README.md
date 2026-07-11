# Server - Modular Agent Framework Unified Backend

## Overview

This is the unified HTTP Server for the Modular Agent Framework, providing a REST API for both Web and VSCode WebView frontends.

**Phase 1 Status**: ✅ Bootstrap & Infrastructure Complete

## Phase 1 Implementation

### What's Included

1. **Express.js 5.2.1 Framework**
   - HTTP server with middleware support
   - CORS handling
   - Request logging
   - Error handling
   - Graceful shutdown

2. **SDK Integration**
   - Complete SDK initialization
   - Configuration loading with environment overrides
   - Storage manager setup
   - Index resolver registration

3. **Dependency Injection Container**
   - Centralized service management
   - Adapter registration and retrieval
   - Service lifecycle management

4. **Configuration System**
   - TOML/JSON configuration support
   - Environment variable overrides
   - Type-safe configuration schema

5. **Logging System**
   - Structured logging
   - File and console output
   - Log level configuration
   - SDK log integration

6. **Graceful Shutdown**
   - Signal handling (SIGTERM, SIGINT)
   - Resource cleanup
   - Timeout management

### File Structure

```
apps/server/
├── src/
│   ├── index.ts                 # Main entry point
│   ├── server.ts                # Express server implementation
│   ├── sdk-bootstrap.ts         # SDK initialization
│   ├── services/
│   │   └── container.ts         # Dependency injection container
│   ├── config/                  # Configuration system (copied from cli-app)
│   ├── storage/                 # Storage management (copied from cli-app)
│   └── utils/                   # Utilities (logger, formatter, output)
├── package.json                 # Dependencies and scripts
├── tsconfig.json                # TypeScript configuration
├── .env.example                 # Environment variable template
└── README.md                    # This file
```

### Dependencies

Key dependencies added in Phase 1:
- `express@^5.2.1` - Web framework
- `@wf-agent/sdk` - Core SDK
- `@wf-agent/config-processor` - Configuration processing
- `@wf-agent/storage` - Storage abstraction
- `dotenv` - Environment variable loading

## Getting Started

### Installation

```bash
cd apps/server
pnpm install
```

### Development

```bash
# Watch mode (recompile on changes)
pnpm dev

# In another terminal, start the server
pnpm start:watch
```

### Building

```bash
# Build TypeScript to JavaScript
pnpm build

# Type checking
pnpm typecheck

# Linting
pnpm lint
pnpm lint:fix

# Formatting
pnpm format
pnpm format:check
```

### Running

```bash
# Start the server
pnpm start

# With custom config
CONFIG_PATH=./custom-config.toml pnpm start

# With custom port
SERVER_PORT=3001 pnpm start
```

## Environment Configuration

Copy `.env.example` to `.env.local`:

```bash
cp .env.example .env.local
```

Configure the following variables:

- `SERVER_PORT` (default: 3000) - HTTP server port
- `SERVER_HOST` (default: 0.0.0.0) - HTTP server host
- `CORS_ORIGINS` - Comma-separated list of allowed CORS origins
- `CONFIG_PATH` - Path to .modular-agent.toml configuration file
- `LOG_LEVEL` - Logging level (debug, info, warn, error)

## Endpoints (Phase 1)

### Health Check
```
GET /health
```

### API Info
```
GET /api/v1/info
```

### Root
```
GET /
```

## Verification

After starting the server, verify it's working:

```bash
# Health check
curl http://localhost:3000/health

# API info
curl http://localhost:3000/api/v1/info

# Root endpoint
curl http://localhost:3000/
```

Expected output:
```json
{
  "status": "ok",
  "timestamp": "2024-07-11T10:00:00.000Z"
}
```

## Architecture

### Initialization Flow

```
1. Environment Setup
   ├── Load .env variables (dotenv)
   └── Initialize output system

2. Configuration Loading
   ├── Load .modular-agent.toml or env overrides
   └── Initialize formatter

3. SDK Bootstrap
   ├── Initialize TOML parser
   ├── Initialize logging system
   ├── Initialize storage manager
   ├── Create SDK instance
   ├── Wait for SDK ready
   └── Register index resolvers

4. Server Initialization
   ├── Initialize dependency container
   ├── Create Express application
   └── Setup middleware and routes

5. Server Start
   ├── Listen on configured port
   └── Setup graceful shutdown handlers
```

### Dependency Container

All major services are managed through `ServerDependencyContainer`:

```typescript
// Register a service
container.registerService('execution', new ExecutionService(sdk));

// Get a service
const service = container.getService<ExecutionService>('execution');
```

## Logging

Logs are output to console and optionally to file. Configure with:

- `LOG_LEVEL` environment variable
- `--verbose` flag for detailed output
- `--debug` flag for debug output

## Error Handling

- **500 Errors**: Server-side errors with stack trace (dev) or generic message (prod)
- **404 Errors**: Endpoint not found
- **CORS Errors**: Check `CORS_ORIGINS` configuration
- **Configuration Errors**: Check `.modular-agent.toml` and environment variables

## Graceful Shutdown

The server handles graceful shutdown on:
- `SIGTERM` signal
- `SIGINT` signal (Ctrl+C)

Shutdown process:
1. Stop accepting new connections
2. Wait for in-flight requests to complete
3. Cleanup SDK resources
4. Close storage connections
5. Exit with status 0

## What's Next (Phase 2+)

- REST API implementation for core resources (Workflow, Tool, Template, etc.)
- Adapter layer for unified resource access
- WebSocket/SSE for real-time events
- Frontend integration (Web, VSCode)
- Testing and optimization

## References

- [Architecture Design](../../docs/apps/server-architecture-design.md)
- [Implementation Roadmap](../../docs/plan/apps/server-implementation-roadmap.md)
- [CLI App Reference](../cli-app) - Reference implementation for architecture
