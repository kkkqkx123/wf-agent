# CLI Storage Integration Issues Analysis

**Date**: 2026-05-08  
**Status**: Analysis Complete - Root Causes Identified  
**Related Document**: [cli-integration-fixes-summary.md](./cli-integration-fixes-summary.md)

## Executive Summary

The CLI application's storage integration has **four critical architectural issues** that cause test failures and potential production problems:

1. **SDK Singleton Caching** - Storage adapters are cached in the SDK singleton, preventing proper isolation between processes
2. **Relative Path Resolution** - STORAGE_DIR environment variable doesn't convert relative paths to absolute paths
3. **Environment Variable Propagation** - Inconsistent propagation of STORAGE_DIR across spawned processes
4. **Storage Manager Lifecycle** - No mechanism to force re-initialization when environment changes

These issues manifest as workflows being registered successfully but not retrievable in subsequent queries within the same test suite.

---

## Issue #1: SDK Singleton Storage Adapter Caching (CRITICAL)

### Problem Description

The SDK uses a global singleton pattern that caches storage adapters on first initialization:

```typescript
// sdk/api/shared/core/sdk.ts
let globalSDK: SDK | null = null;

export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = new SDK(options);  // Only created once!
  }
  return globalSDK;
}
```

When `getSDK()` is called with storage adapters:

```typescript
// apps/cli-app/src/index.ts (line 111-119)
const sdk = getSDK({
  checkpointStorageAdapter: storageManager?.getCheckpointStorage() ?? undefined,
  workflowStorageAdapter: storageManager?.getWorkflowStorage() ?? undefined,
  taskStorageAdapter: storageManager?.getTaskStorage() ?? undefined,
  workflowExecutionStorageAdapter: storageManager?.getWorkflowExecutionStorage() ?? undefined,
  agentLoopCheckpointStorageAdapter: storageManager?.getAgentLoopCheckpointStorage() ?? undefined,
});
```

**The Problem**: 
- The SDK constructor stores options in `this.pendingOptions`
- During bootstrap, it calls `initializeContainerWithAdapters()` with these adapters
- The DI container binds these adapters as **singletons**
- On subsequent CLI process spawns, even with different `STORAGE_DIR`, the SDK singleton may already exist and reuse old adapters

### Evidence from Code

```typescript
// sdk/core/di/container-config.ts (line 121-127)
export function initializeContainerWithAdapters(adapters: StorageAdapterConfig = {}): Container {
  if (container) {
    return container;  // Returns existing container without rebinding!
  }
  
  container = new Container();
  pendingStorageConfig = adapters;
  // ... bindings happen only once
}
```

### Impact

- **Test Environment**: Multiple test cases spawning CLI processes with different `STORAGE_DIR` values may share the same storage instance
- **Production**: If CLI is used in scenarios where storage location changes dynamically, old storage will be reused

### Verification Test

See `storage-isolation.test.ts` → "Issue #1: SDK Singleton Storage Caching"

---

## Issue #2: Relative vs Absolute Path Resolution (MODERATE)

### Problem Description

The `loadConfigWithEnvOverride()` function applies `STORAGE_DIR` directly without resolving to absolute path:

```typescript
// apps/cli-app/src/config/cli/loader.ts (line 91-94)
if (process.env["STORAGE_DIR"] && config.storage?.json) {
  config.storage.json = { ...config.storage.json, baseDir: process.env["STORAGE_DIR"] };
}
```

**The Problem**:
- If `STORAGE_DIR=./relative/path` is set, it remains relative
- When the working directory changes (e.g., tests run from different directories), the path resolves differently
- This causes inconsistent behavior depending on where the CLI is invoked from

### Example Scenario

```bash
# From /project root
STORAGE_DIR=./storage node cli.js workflow list
# Resolves to: /project/storage ✓

# From /project/apps/cli-app
STORAGE_DIR=./storage node cli.js workflow list  
# Resolves to: /project/apps/cli-app/storage ✗ (different location!)
```

### Impact

- **Test Environment**: Tests running from different working directories may access different storage locations
- **Production**: Scripts that change directories before invoking CLI will see inconsistent data

### Verification Test

See `storage-isolation.test.ts` → "Issue #2: Path Resolution Problems"

---

## Issue #3: Environment Variable Propagation Across Processes (MODERATE)

### Problem Description

The `CLIRunner` utility sets `STORAGE_DIR` in the environment for spawned processes:

```typescript
// apps/cli-app/__tests__/utils/cli-runner.ts (line 89-93)
const env = { ...this.defaultEnv, ...options.env };
if (this.storageDir && !env["STORAGE_DIR"]) {
  env["STORAGE_DIR"] = this.storageDir;
}
```

**The Problem**:
- Each test spawns a new Node.js process via `child_process.spawn()`
- The environment is passed correctly to the child process
- However, if the SDK singleton persists in memory (in some edge cases), or if there are nested spawns, the environment may not propagate correctly
- There's no validation that the environment variable was actually received and used

### Additional Complexity

The CLI initialization flow:
1. Process starts → reads `STORAGE_DIR` from env
2. Loads config → applies `STORAGE_DIR` override
3. Initializes StorageManager → creates storage instances
4. Initializes SDK → passes storage adapters to SDK
5. SDK bootstraps → binds adapters in DI container

If any step fails to properly read or apply the environment variable, isolation breaks.

### Impact

- **Test Environment**: Rapid sequential test execution may experience cross-contamination
- **Production**: Less likely to be an issue unless CLI spawns child processes internally

### Verification Test

See `storage-isolation.test.ts` → "Issue #3: Environment Variable Propagation"

---

## Issue #4: Storage Manager Initialization Timing (LOW-MODERATE)

### Problem Description

The `StorageManager` is initialized once per CLI process in the `preAction` hook:

```typescript
// apps/cli-app/src/index.ts (line 107-108)
await initializeStorageManager(config);
const storageManager = getStorageManager();
```

The global storage manager is also a singleton:

```typescript
// apps/cli-app/src/storage/storage-manager.ts (line 263-283)
let globalStorageManager: StorageManager | null = null;

export async function initializeStorageManager(config: CLIConfig): Promise<StorageManager> {
  if (globalStorageManager) {
    return globalStorageManager;  // Returns existing instance!
  }
  
  globalStorageManager = new StorageManager(config);
  await globalStorageManager.initialize();
  return globalStorageManager;
}
```

**The Problem**:
- Within a single CLI process, this works fine
- But combined with Issue #1 (SDK singleton), if somehow the SDK persists across process boundaries (e.g., through worker threads or other mechanisms), the StorageManager singleton would also persist
- There's no mechanism to force re-initialization with new configuration

### Impact

- **Test Environment**: Could compound Issue #1 if both singletons persist
- **Production**: Low risk unless using advanced process management features

### Verification Test

See `storage-isolation.test.ts` → "Issue #4: Storage Manager Lifecycle"

---

## Root Cause Analysis

### Why These Issues Exist

1. **Singleton Pattern Misuse**: Both SDK and StorageManager use singletons optimized for long-running applications, but CLI is designed for short-lived processes. The singleton assumption breaks down in test environments that spawn multiple processes.

2. **No Process Boundary Awareness**: The code assumes each CLI invocation is completely isolated, but doesn't account for:
   - Node.js module caching across test runs
   - Potential worker thread usage
   - Test frameworks that may keep processes alive

3. **Lack of Validation**: No runtime checks verify that:
   - The correct storage directory is being used
   - Storage adapters are pointing to the expected location
   - Environment variables were properly applied

4. **Configuration Override Order**: The environment variable override happens after config loading, but there's no guarantee that all components respect the override.

---

## Recommended Solutions

### Solution A: Force Storage Re-initialization Per Process (RECOMMENDED)

**Approach**: Add a mechanism to reset SDK and StorageManager singletons based on environment changes.

**Implementation**:

```typescript
// apps/cli-app/src/index.ts - Add before SDK initialization
import { resetSDK, resetStorageManager } from "./lifecycle/reset.js";

// In preAction hook, check if storage dir changed
const currentStorageDir = process.env["STORAGE_DIR"];
const lastStorageDir = getLastUsedStorageDir();

if (lastStorageDir && lastStorageDir !== currentStorageDir) {
  // Reset singletons to force re-initialization
  await resetSDK();
  await resetStorageManager();
}

setLastUsedStorageDir(currentStorageDir);
```

**Pros**:
- Minimal code changes
- Maintains singleton benefits within a process
- Explicit control over when to reset

**Cons**:
- Requires tracking state across invocations
- May have performance impact if resetting frequently

---

### Solution B: Use Absolute Paths Always (REQUIRED)

**Approach**: Always resolve `STORAGE_DIR` to absolute path immediately.

**Implementation**:

```typescript
// apps/cli-app/src/config/cli/loader.ts
import { resolve } from "path";

export async function loadConfigWithEnvOverride(configPath?: string): Promise<CLIConfig> {
  const config = await loadConfig(configPath);

  // Apply STORAGE_DIR override with absolute path resolution
  if (process.env["STORAGE_DIR"] && config.storage?.json) {
    const absolutePath = resolve(process.cwd(), process.env["STORAGE_DIR"]);
    config.storage.json = { ...config.storage.json, baseDir: absolutePath };
  }

  return config;
}
```

**Pros**:
- Simple fix
- Eliminates path ambiguity
- Works regardless of working directory

**Cons**:
- Doesn't solve the singleton caching issue alone

---

### Solution C: In-Memory Storage for Tests (TESTING ONLY)

**Approach**: Configure tests to use in-memory storage instead of file-based storage.

**Implementation**:

```typescript
// apps/cli-app/__tests__/utils/cli-runner.ts
this.defaultEnv = {
  ...process.env,
  NODE_ENV: "test",
  TEST_MODE: "true",
  USE_IN_MEMORY_STORAGE: "true",  // New flag
  // ... other env vars
};
```

Then modify StorageManager to support in-memory mode:

```typescript
// apps/cli-app/src/storage/storage-manager.ts
if (process.env["USE_IN_MEMORY_STORAGE"] === "true") {
  // Use in-memory adapters
  this.workflowStorage = new InMemoryWorkflowStorage();
  // ... etc
}
```

**Pros**:
- Completely eliminates file system timing issues
- Fastest test execution
- Perfect isolation between tests

**Cons**:
- Doesn't test actual file-based storage behavior
- Requires implementing in-memory adapters
- Not applicable to production

---

### Solution D: Process-Level Isolation with Unique IDs (COMPREHENSIVE)

**Approach**: Generate unique storage directory for each CLI invocation in test mode.

**Implementation**:

```typescript
// apps/cli-app/__tests__/utils/cli-runner.ts
async run(args: string[], options: CLIRunOptions = {}): Promise<CLIRunResult> {
  // Generate unique storage dir for this invocation
  const uniqueStorageDir = join(
    this.storageDir || tempBaseDir,
    `invocation-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(uniqueStorageDir, { recursive: true });

  const env = { 
    ...this.defaultEnv, 
    ...options.env,
    STORAGE_DIR: uniqueStorageDir,  // Always override
  };

  // ... spawn process
}
```

**Pros**:
- Guaranteed isolation
- No singleton conflicts possible
- Easy to debug (each invocation has its own directory)

**Cons**:
- More disk space usage
- Need cleanup logic
- May mask real issues that would occur in production

---

## Implementation Priority

### Phase 1: Critical Fixes (Immediate)

1. ✅ **Implement Solution B** - Absolute path resolution
   - File: `apps/cli-app/src/config/cli/loader.ts`
   - Effort: ~10 minutes
   - Risk: Very low

2. ✅ **Create Integration Tests** - Verify issues exist
   - File: `apps/cli-app/__tests__/integration/storage-isolation.test.ts`
   - Effort: Already done
   - Risk: None (tests only)

### Phase 2: Core Fixes (Short-term)

3. 🔄 **Implement Solution A** - Force re-initialization
   - Files: `apps/cli-app/src/lifecycle/reset.ts` (new), `apps/cli-app/src/index.ts`
   - Effort: ~2 hours
   - Risk: Medium (need to ensure proper cleanup)

4. 🔄 **Add Runtime Validation** - Log storage directory being used
   - File: `apps/cli-app/src/storage/storage-manager.ts`
   - Effort: ~30 minutes
   - Risk: Very low

### Phase 3: Testing Improvements (Medium-term)

5. ⏳ **Implement Solution D** - Unique storage per test
   - File: `apps/cli-app/__tests__/utils/cli-runner.ts`
   - Effort: ~1 hour
   - Risk: Low

6. ⏳ **Consider Solution C** - In-memory storage for specific tests
   - Files: Multiple (new in-memory adapters needed)
   - Effort: ~4-6 hours
   - Risk: Medium (new code to maintain)

---

## Monitoring and Debugging

### Diagnostic Logging

Add this to help identify which storage directory is being used:

```typescript
// apps/cli-app/src/storage/storage-manager.ts
async initialize(): Promise<void> {
  logger.info("StorageManager initializing", {
    storageType: this.config.storage?.type,
    baseDir: this.config.storage?.json?.baseDir,
    dbPath: this.config.storage?.sqlite?.dbPath,
    cwd: process.cwd(),
    storageDirEnv: process.env["STORAGE_DIR"],
  });
  
  // ... rest of initialization
}
```

### Health Check Command

Create a diagnostic command:

```bash
modular-agent debug storage-info
```

Output:
```
Storage Configuration:
  Type: json
  Base Directory: /absolute/path/to/storage
  Working Directory: /current/working/dir
  STORAGE_DIR Env: /value/from/environment
  Initialized: true
  Workflow Count: 5
  Execution Count: 12
```

---

## Conclusion

The storage integration issues stem from a fundamental mismatch between:
- **Singleton architecture** (designed for long-running services)
- **CLI execution model** (short-lived, potentially parallel processes)

The fixes require:
1. **Immediate**: Absolute path resolution (Solution B)
2. **Short-term**: Singleton reset mechanism (Solution A)
3. **Testing**: Enhanced isolation strategies (Solutions C/D)

Once these fixes are implemented, the 5 failing integration tests should pass, and the CLI will have robust storage isolation suitable for both testing and production use.

---

## Related Files

- **Analysis**: `docs/tests/summary/cli-integration-fixes-summary.md`
- **Tests**: `apps/cli-app/__tests__/integration/storage-isolation.test.ts`
- **Config Loader**: `apps/cli-app/src/config/cli/loader.ts`
- **Storage Manager**: `apps/cli-app/src/storage/storage-manager.ts`
- **CLI Entry Point**: `apps/cli-app/src/index.ts`
- **SDK Singleton**: `sdk/api/shared/core/sdk.ts`
- **DI Container**: `sdk/core/di/container-config.ts`
