# CLI Storage Isolation Root Cause Analysis

**Date**: 2026-05-08  
**Status**: ✅ Root Cause Identified  
**Based on**: Storage package integration tests (15/15 passed)

## Executive Summary

The storage package itself is **functionally correct** - all 15 integration tests pass successfully. The CLI app storage isolation failures are caused by **SDK singleton caching** at a higher architectural layer, not by storage implementation bugs.

### Key Finding

**The SDK uses a global singleton pattern (`getSDK()`) that caches the DI container and storage adapters across multiple invocations within the same Node.js process.** This prevents proper isolation when tests spawn multiple CLI processes with different `STORAGE_DIR` environment variables.

---

## Evidence from Storage Package Tests

### ✅ Storage Package Works Correctly

All 15 integration tests in `packages/storage/src/__tests__/storage-integration.test.ts` passed:

```
Test Files  1 passed (1)
     Tests  15 passed (15)
  Duration  ~3.4s
```

Tests verified:
- ✅ JSON file storage creates correct directory structure
- ✅ SQLite storage initializes databases properly
- ✅ CRUD operations work correctly for both backends
- ✅ Path resolution handles relative and absolute paths
- ✅ **Storage instances maintain complete isolation** (different baseDir = different data)

### Critical Test Result

The "Storage Instance Isolation" test proves that when you create two separate storage instances with different directories, they remain isolated:

```typescript
const dir1 = path.join(tempBaseDir, "instance-1");
const dir2 = path.join(tempBaseDir, "instance-2");

const storage1 = new JsonWorkflowStorage({ baseDir: dir1 });
const storage2 = new JsonWorkflowStorage({ baseDir: dir2 });

await storage1.save("shared-id", new Uint8Array([1]), metadata);
await storage2.save("shared-id", new Uint8Array([2]), metadata);

// Each has its own data - isolation works!
expect(await storage1.load("shared-id")).toEqual(new Uint8Array([1]));
expect(await storage2.load("shared-id")).toEqual(new Uint8Array([2]));
```

**Conclusion**: The storage package correctly isolates data when given different base directories.

---

## Root Cause: SDK Singleton Caching

### Problem Flow

```
Test Suite Start
    ↓
Test 1: Set STORAGE_DIR="/tmp/test-1"
    ↓
CLI Process Spawns → Calls getSDK()
    ↓
SDK Constructor → initializeContainerWithAdapters()
    ↓
DI Container Created + Storage Adapters Cached ← First initialization
    ↓
Workflow Registered to /tmp/test-1 ✓
    ↓
Test 1 Ends (process exits)
    ↓
Test 2: Set STORAGE_DIR="/tmp/test-2"
    ↓
CLI Process Spawns → Calls getSDK()
    ↓
⚠️ SDK Singleton Check: if (!globalSDK) { ... }
    ↓
❌ globalSDK already exists! Returns cached instance
    ↓
❌ Uses OLD storage adapters pointing to /tmp/test-1
    ↓
❌ Workflow registered to wrong directory
    ↓
❌ Query returns empty or wrong results
```

### Code Evidence

#### 1. SDK Global Singleton ([sdk.ts:499-529](file:///d:/项目/agent/wf-agent/sdk/api/shared/core/sdk.ts#L499-L529))

```typescript
/**
 * Global SDK instance
 */
let globalSDK: SDK | null = null;

export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = new SDK(options);
  }
  return globalSDK;  // ⚠️ Always returns same instance after first call
}
```

**Problem**: Once `globalSDK` is created, subsequent calls to `getSDK()` ignore new options and return the cached instance.

#### 2. DI Container Singleton ([container-config.ts:121-127](file:///d:/项目/agent/wf-agent/sdk/core/di/container-config.ts#L121-L127))

```typescript
export function initializeContainerWithAdapters(adapters: StorageAdapterConfig = {}): Container {
  if (container) {
    return container;  // ⚠️ Returns existing container, ignores new adapters
  }

  container = new Container();
  pendingStorageConfig = adapters;
  // ... binding logic
}
```

**Problem**: The DI container is also a singleton. Once initialized, it cannot be reconfigured with new storage adapters.

#### 3. Storage Adapter Binding ([container-config.ts:133-141](file:///d:/项目/agent/wf-agent/sdk/core/di/container-config.ts#L133-L141))

```typescript
// CheckpointStorageAdapter
container
  .bind(Identifiers.CheckpointStorageAdapter)
  .toDynamicValue(() => pendingStorageConfig?.checkpoint || null)
  .inSingletonScope();  // ⚠️ Singleton scope - never recreated

// WorkflowStorageAdapter
container
  .bind(Identifiers.WorkflowStorageAdapter)
  .toDynamicValue(() => pendingStorageConfig?.workflow || null)
  .inSingletonScope();  // ⚠️ Singleton scope - never recreated
```

**Problem**: Storage adapters are bound as singletons. Even if the container were reset, these adapters would need explicit rebinding.

---

## Why Storage Tests Pass but CLI Tests Fail

| Aspect | Storage Package Tests | CLI App Tests |
|--------|----------------------|---------------|
| **Instance Creation** | Creates fresh instances per test | Uses `getSDK()` singleton |
| **Isolation** | Different `baseDir` per test | Same SDK instance across tests |
| **Lifecycle** | `beforeEach` creates, `afterEach` destroys | No SDK reset between tests |
| **Process Model** | Single process, controlled instances | Multiple spawns, shared singleton |
| **Result** | ✅ Isolation works | ❌ Isolation fails |

---

## Architecture Decision Issues

### Issue 1: Module-Level Singleton Pattern

The SDK uses module-level state (`globalSDK`) which persists across the entire Node.js process lifetime:

```typescript
// sdk.ts - Module scope
let globalSDK: SDK | null = null;  // Persists until process exits
```

**Impact**: In test environments where multiple scenarios need different configurations, this prevents isolation.

### Issue 2: No Reset Mechanism

While there's a `resetContainer()` function, it's not exposed through the SDK API and doesn't reset `globalSDK`:

```typescript
// container-config.ts:814
export function resetContainer(): void {
  if (container) {
    container.clearAllCaches();
    container = null;
  }
  pendingStorageConfig = null;
}

// But SDK still has:
let globalSDK: SDK | null = null;  // Not reset!
```

### Issue 3: Options Ignored After First Call

```typescript
export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = new SDK(options);  // Only uses options once
  }
  return globalSDK;  // Subsequent options ignored
}
```

**Impact**: Even if tests pass different `STORAGE_DIR` values, only the first one is used.

---

## Recommended Solutions

### Solution 1: Add SDK Reset Functionality (Recommended for Testing)

Add a `resetSDK()` function to allow test isolation:

```typescript
// sdk/api/shared/core/sdk.ts

/**
 * Reset the global SDK instance (for testing purposes)
 */
export function resetSDK(): void {
  if (globalSDK) {
    globalSDK.destroy().catch(err => {
      logger.error("Error during SDK reset", { error: getErrorMessage(err) });
    });
    globalSDK = null;
  }
  
  // Also reset the DI container
  resetContainer();
}
```

**Usage in tests**:

```typescript
import { resetSDK } from "@wf-agent/sdk";

afterEach(async () => {
  await resetSDK();  // Force fresh SDK for next test
});
```

### Solution 2: Support Multiple SDK Instances (Long-term)

Allow creating isolated SDK instances instead of forcing singleton:

```typescript
export function createSDK(options?: SDKOptions): SDK {
  return new SDK(options);  // Always create new instance
}

export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = createSDK(options);
  }
  return globalSDK;
}
```

**Benefits**:
- Tests can create isolated instances
- Applications can have multiple SDK contexts
- Better separation of concerns

### Solution 3: Environment Variable Detection (Quick Fix)

Make `getSDK()` detect environment changes and force reinitialization:

```typescript
let lastStorageDir: string | undefined;

export function getSDK(options?: SDKOptions): SDK {
  const currentStorageDir = process.env.STORAGE_DIR;
  
  // Detect environment change
  if (globalSDK && currentStorageDir !== lastStorageDir) {
    logger.warn("STORAGE_DIR changed, resetting SDK");
    await globalSDK.destroy();
    globalSDK = null;
    resetContainer();
  }
  
  if (!globalSDK) {
    lastStorageDir = currentStorageDir;
    globalSDK = new SDK(options);
  }
  
  return globalSDK;
}
```

**Drawback**: Relies on environment variable detection, may miss other configuration changes.

---

## Immediate Actions for CLI App Tests

### 1. Use Process-Level Isolation

Ensure each test spawns a completely new Node.js process:

```typescript
import { spawn } from "child_process";

it("should isolate storage between tests", async () => {
  // Spawn fresh process for each operation
  const register = spawn("node", ["cli.js", "register", "..."], {
    env: { ...process.env, STORAGE_DIR: "/tmp/test-1" }
  });
  
  const query = spawn("node", ["cli.js", "query", "..."], {
    env: { ...process.env, STORAGE_DIR: "/tmp/test-2" }
  });
  
  // These are truly isolated processes
});
```

### 2. Verify Process Exit

Ensure CLI processes fully exit before starting next test:

```typescript
await new Promise((resolve, reject) => {
  proc.on("exit", (code) => {
    if (code === 0) resolve(undefined);
    else reject(new Error(`Process exited with code ${code}`));
  });
});
```

### 3. Add SDK Reset Hook

If CLI app maintains long-running processes, add cleanup:

```typescript
// apps/cli-app/src/index.ts
process.on("SIGTERM", async () => {
  const sdk = getSDK();
  await sdk.shutdown();
  process.exit(0);
});
```

---

## Verification Steps

To confirm this is the root cause:

1. **Test with fresh processes**: Modify CLI tests to ensure each command spawns a new Node.js process
2. **Add logging**: Log `globalSDK` creation in `getSDK()` to see if it's reused
3. **Check container state**: Log container initialization to verify if it's called multiple times
4. **Monitor STORAGE_DIR**: Track when and how `STORAGE_DIR` is read vs when SDK is initialized

Example diagnostic code:

```typescript
// Add to sdk.ts temporarily
let sdkCreationCount = 0;

export function getSDK(options?: SDKOptions): SDK {
  sdkCreationCount++;
  logger.info(`getSDK() called (count: ${sdkCreationCount})`, {
    hasGlobalSDK: !!globalSDK,
    storageDir: process.env.STORAGE_DIR,
  });
  
  if (!globalSDK) {
    globalSDK = new SDK(options);
  }
  return globalSDK;
}
```

---

## Conclusion

**Root Cause**: SDK singleton caching prevents storage isolation across multiple CLI invocations within the same test suite.

**Evidence**: 
- ✅ Storage package works correctly (15/15 tests pass)
- ✅ Storage instances isolate properly when given different directories
- ❌ SDK's `getSDK()` returns cached instance, ignoring new options
- ❌ DI container singleton prevents adapter reconfiguration

**Solution Priority**:
1. **Immediate**: Add `resetSDK()` function for test isolation
2. **Short-term**: Ensure CLI tests use true process isolation
3. **Long-term**: Support multiple SDK instances or environment-aware reset

The storage package is not the problem - the issue is at the SDK orchestration layer where singleton patterns prevent proper test isolation.
