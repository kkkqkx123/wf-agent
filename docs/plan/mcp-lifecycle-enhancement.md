# MCP Server Lifecycle Enhancement

## Context

Currently MCP servers have only `disabled`/`timeout` config. There is no lifecycle strategy — all servers are implicitly "manual" (need explicit `connectServer()` call), never auto-disconnect, and never auto-reconnect.

This limits resource usage (too many long-lived connections) and reliability (no health-check-based recovery).

## Design

### 1. New Config Fields

Add lifecycle fields at two levels:

#### Server-level (`McpServerConfigBase`)

```typescript
export interface McpServerConfigBase {
  /** Server type */
  type?: McpTransportType;
  /** Whether the server is disabled */
  disabled?: boolean;
  /** Timeout in seconds (1-3600, default 60) */
  timeout?: number;

  // NEW fields
  /**
   * Connection lifecycle strategy:
   * - lazy (default) — Don't connect at startup. Connect on first tool call.
   *   Disconnect after idle timeout. Cached metadata keeps search/list working.
   * - eager — Connect at startup but don't auto-reconnect if the connection drops.
   *   No idle timeout by default (set idleTimeout explicitly to enable).
   * - keep-alive — Connect at startup. Auto-reconnect via health checks.
   *   No idle timeout. Use for servers needed always.
   */
  lifecycle?: "lazy" | "eager" | "keep-alive";
  /** Idle timeout in seconds (0 = no idle timeout). Only applicable for lazy/eager. */
  idleTimeout?: number;
  /** Keep-alive health check interval in seconds (default: 30). Only for keep-alive mode. */
  healthCheckInterval?: number;
}
```

#### Manager-level (`McpManagerOptions`)

```typescript
export interface McpManagerOptions {
  mcpEnabled?: boolean;
  maxErrorHistory?: number;
  connectionTimeout?: number;
  configDebounceDelay?: number;

  // NEW fields
  defaultLifecycle?: "lazy" | "eager" | "keep-alive";
  defaultIdleTimeout?: number;
  defaultHealthCheckInterval?: number;
}
```

### 2. Connection Manager Changes

In `McpConnectionManager`:

- **`connectServer()`**: Respect `lifecycle` field. For `eager`/`keep-alive`, connect immediately. For `lazy`, only register metadata (no connect).
- **`callTool()`**: If server is in `lazy` mode and disconnected, auto-connect before the call.
- **`idleTimeout` timer**: After each tool call, reset and start an idle timer. On timeout, disconnect the server (unless `keep-alive`).
- **Health check loop**: For `keep-alive` servers, periodically ping the server. On failure, attempt reconnect.
- **`disconnectAll()`**: Clean up all timers/intervals.

### 3. Server Registry Changes

In `McpServerRegistry`:

- After `McpConnectionManager` is created, eager/keep-alive servers must be connected.
- When `getInstance()` is called, trigger connection for servers whose lifecycle strategy requires startup connection.

### 4. Config Processor

In `mcp-connection-processor.ts`:

- `resolveServerLifecycle()`: Merge server-level lifecycle with manager-level defaults.
- `validateLifecycleConfig()`: Validate field combinations (e.g., `idleTimeout` only works with `lazy`/`eager`).

## Data Flow

```
McpManagerOptions.defaultLifecycle
        │
        ▼
loadServerConfigs() ──► mergeServerConfigs()
        │                         │
        └─── resolveServerLifecycle(server, defaults)
                        │
                        ▼
              McpConnectionManager.connectServer()
                  ├── eager ──────→ connect immediately
                  ├── keep-alive ─→ connect + health check loop
                  └── lazy ───────→ register only, connect on first use
```

## Files to Change

| File | Change |
|------|--------|
| `packages/types/src/tool/mcp-connection.ts` | Add lifecycle fields to `McpServerConfigBase`, `McpManagerOptions` |
| `sdk/api/shared/types/core-types.ts` | Add default lifecycle fields to `McpConfig` |
| `sdk/services/mcp/mcp-connection-processor.ts` | Add lifecycle resolution/validation functions |
| `sdk/services/mcp/connection-manager.ts` | Implement lazy auto-connect, idle timeout, health check |
| `sdk/services/mcp/server-registry.ts` | Trigger startup connections for eager/keep-alive |
| `sdk/services/mcp/connection-state.ts` | Add state helpers for lifecycle tracking |
| `sdk/api/shared/core/sdk-instance.ts` | Pass default lifecycle config to MCP manager |