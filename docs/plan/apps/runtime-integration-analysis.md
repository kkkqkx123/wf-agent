# Runtime Integration Analysis - SDK & Apps Integration Refactoring

## 1. Background

The Modular Agent Framework currently has two applications (`apps/cli-app` and `apps/server`) that directly integrate with the SDK (`packages/sdk`). Both apps share a significant amount of "wiring" code — SDK initialization, storage management, logger configuration, and lifecycle management — but implement it independently, leading to substantial duplication.

## 2. Current State Analysis

### 2.1 Duplication Quantification

| File / Module | cli-app | server | Status |
|--------------|---------|--------|--------|
| StorageManager | `src/storage/storage-manager.ts` | `src/storage/storage-manager.ts` | **100% identical** (incl. `cli-app` logger name) |
| Logger init | `src/utils/logger.ts` | `src/utils/logger.ts` | **100% identical** |
| SDK bootstrap | Inline in `index.ts` preAction | `src/sdk-bootstrap.ts` | Logically identical, organized differently |
| BaseAdapter | `src/adapters/base-adapter.ts` | `src/adapters/base-adapter.ts` | **Completely different** design |
| Container | `src/services/container.ts` | `src/services/container.ts` | **Completely different** design |
| Config system | `src/config/cli/` | `src/config/cli/` | Nearly identical |
| Output system | `src/utils/output.ts` | `src/utils/output.ts` | Nearly identical |
| Formatter | `src/utils/formatter.ts` | `src/utils/formatter.ts` | Nearly identical |
| ExitManager | `src/utils/exit-manager.ts` | `src/utils/exit-manager.ts` | Nearly identical |

**Result**: Of server's 29 source files, ~20 are derived from cli-app. Only 7 are server-specific (`server.ts`, 4 route files, `event-manager.ts`, `api-response.ts`).

### 2.2 Integration Flow (Both Apps)

```
loadConfigWithEnvOverride()
  → new StorageManager(config).initialize()
    → createSDK({ ...storageManager.getAllAdapters(), ... })
      → sdkInstance.waitForReady()
        → registerAllIndexResolvers()
          → initContainer(sdk, config)
```

### 2.3 Server Adapter Status

The server's `workflow-adapter.ts` contains **only TODO stub implementations**:

```typescript
private async getWorkflowsFromSDK(): Promise<Workflow[]> {
  // TODO: Implement SDK integration
  return [];
}
```

The server is currently non-functional for actual workflow operations.

## 3. Architecture Options

### Option A: Server as the Integration Hub (Rejected)

```
CLI (thin) ──HTTP──→ Server ──→ SDK
Web UI      ──HTTP──→ Server ──→ SDK
```

**Why rejected:**
- CLI would require a local HTTP server process to function
- CLI's `ExecutionService` relies on `node-pty` for real-time terminal streaming — impossible over HTTP
- TUI mode requires in-process SDK access
- Single-binary deployment becomes impossible
- Debugging complexity increases dramatically
- CLI is a local dev tool; adding a server dependency defeats its purpose

### Option B: Extract Shared Runtime Package (Recommended) ✅

```
cli-app ──→ @wf-agent/runtime ──→ SDK
server  ──→ @wf-agent/runtime ──→ SDK
Web UI  ──→ server ──→ @wf-agent/runtime ──→ SDK
```

### Option C: Server-Core Library (Not Recommended)

```
cli-app ──→ @wf-agent/server-core (library) ──→ SDK
server  ──→ @wf-agent/server-core (library) ──→ SDK (HTTP layer)
```

**Why rejected:** Semantic confusion — "server-core" suggests server-specific code, but CLI depends on it.

## 4. Recommended Architecture: `@wf-agent/runtime`

### 4.1 Package Responsibilities

| Module | Contents | Source |
|--------|----------|--------|
| `bootstrap/` | `createAppSDK()` — unified SDK creation & initialization | Extracted from both apps |
| `storage/` | `StorageManager` — unified storage adapter management | Extracted from both apps (identical) |
| `logger/` | `initLogger()`, `initSDKLogger()` — standardized logger config | Extracted from both apps (identical) |
| `lifecycle/` | Graceful shutdown, signal handling, resource cleanup | Extracted from both apps |
| `adapters/` | `BaseAppAdapter` — shared base class for app adapters | New design |
| `config/` | Shared configuration types and loading utilities | Extracted from both apps |

### 4.2 Package Structure

```
packages/runtime/
├── src/
│   ├── bootstrap/
│   │   ├── create-app-sdk.ts
│   │   └── index.ts
│   ├── storage/
│   │   ├── storage-manager.ts
│   │   └── index.ts
│   ├── logger/
│   │   ├── init-logger.ts
│   │   └── index.ts
│   ├── lifecycle/
│   │   ├── graceful-shutdown.ts
│   │   └── index.ts
│   ├── adapters/
│   │   ├── base-adapter.ts
│   │   └── index.ts
│   ├── config/
│   │   ├── types.ts
│   │   └── index.ts
│   └── index.ts
├── package.json
├── tsconfig.json
└── vitest.config.mjs
```

### 4.3 Dependencies

```json
{
  "dependencies": {
    "@wf-agent/sdk": "workspace:*",
    "@wf-agent/storage": "workspace:*",
    "@wf-agent/common-utils": "workspace:*",
    "@wf-agent/types": "workspace:*",
    "@wf-agent/config-processor": "workspace:*"
  }
}
```

### 4.4 Adapter Design Decision

CLI and Server adapters have **fundamentally different needs**:

| Dimension | CLI Adapter | Server Adapter |
|-----------|-------------|----------------|
| Output | Formatted text, tables, JSON | JSON HTTP responses |
| Error handling | Display to terminal user | Return HTTP status codes |
| Pagination | Not needed | Required (`applyPagination()`) |
| Interaction | User prompts needed | Pure API calls |
| Data flow | Stream to terminal | SSE to HTTP client |

**Decision**: `@wf-agent/runtime` provides a minimal `BaseAppAdapter` that only handles SDK instance access and basic error wrapping. CLI and Server each extend it with their own semantics.

### 4.5 Migration Impact

| Package | Files Removed | Files Changed | New Dependencies |
|---------|---------------|---------------|------------------|
| cli-app | `src/storage/storage-manager.ts`<br>`src/utils/logger.ts` (partially) | `src/index.ts`<br>`src/adapters/base-adapter.ts` | `@wf-agent/runtime` |
| server | `src/storage/storage-manager.ts`<br>`src/utils/logger.ts` (partially)<br>`src/sdk-bootstrap.ts` | `src/index.ts`<br>`src/adapters/base-adapter.ts` | `@wf-agent/runtime` |
| New | — | — | `packages/runtime/` |

## 5. Implementation Roadmap

### Phase 1: Package Creation
- Create `packages/runtime/` with `package.json`, `tsconfig.json`
- Migrate `StorageManager` from cli-app
- Migrate `initLogger`/`initSDKLogger` from cli-app
- Create `createAppSDK()` bootstrap function
- Create shared `BaseAppAdapter`

### Phase 2: cli-app Migration
- Update cli-app to import from `@wf-agent/runtime`
- Remove local `storage-manager.ts`
- Simplify `index.ts` bootstrap logic

### Phase 3: server Migration
- Update server to import from `@wf-agent/runtime`
- Remove local `storage-manager.ts`
- Replace `sdk-bootstrap.ts` with runtime calls
- Implement server-specific adapters (currently stubs)

### Phase 4: Cleanup
- Remove duplicated config/utils files from server
- Verify both apps build and pass tests
- Update documentation

## 6. Future Considerations

- **New app types**: A batch-processing script or test harness can now use `@wf-agent/runtime` for consistent SDK integration
- **SDK version upgrades**: Only `@wf-agent/runtime` needs to be updated, not each app individually
- **Testing**: Shared bootstrap logic can be tested in one place
- **Configuration**: Centralized config schema validation prevents drift between apps