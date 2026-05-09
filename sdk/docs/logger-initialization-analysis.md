# SDK Logger Initialization Analysis

## Overview

This document analyzes the CLI-APP's control over SDK logger configuration and verifies that initialization order does not cause loss of control.

## Initialization Flow

### Correct Initialization Sequence

```typescript
// apps/cli-app/src/index.ts

// Step 1: Initialize output manager (creates log file path)
const output = initializeOutput({...});

// Step 2: Initialize CLI logger
initLogger({
  verbose, debug, logFile, ...
});

// Step 3: Initialize SDK logger with configuration
initSDKLogger({
  verbose, debug, logFile, ...
});
// ↓ This calls configureSDKLogger() which sets pendingSDKConfig
// ↓ But does NOT create logger instances yet

// Step 4: Initialize SDK (which may access loggers during bootstrap)
const sdk = getSDK({...});
// ↓ SDK constructor starts async bootstrap
// ↓ Bootstrap calls logger.error() or logger.info()
// ↓ Proxy intercepts and calls getSDKLoggerInstance()
// ↓ getSDKLoggerInstance() calls initializeSDKLogger()
// ↓ initializeSDKLogger() uses pendingSDKConfig ✅
```

## Key Design Pattern: Pending Configuration

The SDK logger uses a **pending configuration pattern** to ensure proper initialization order:

### How It Works

1. **Configuration Phase** (`configureSDKLogger`):
   - Sets `pendingSDKConfig`, `pendingGraphConfig`, `pendingAgentConfig`
   - Does NOT create logger instances
   - If loggers already exist, updates them immediately

2. **Initialization Phase** (first logger access):
   - Proxy intercepts method call
   - Calls `getSDKLoggerInstance()`
   - Checks if instance exists
   - If not, calls `initializeSDKLogger()`
   - `initializeSDKLogger()` uses: `config?.level ?? pendingSDKConfig?.level ?? getSDKLogLevel()`
   - Creates logger with correct configuration

### Code Flow

```typescript
// sdk/utils/logger.ts

// Phase 1: Configuration (called by CLI)
export function configureSDKLogger(config: {...}): void {
  pendingSDKConfig = {
    level: config.sdkLevel ?? config.level,
    stream: config.stream,
  };
  // ... similar for graph and agent
  
  // If loggers already exist, update them
  if (sdkLoggerInstance) {
    sdkLoggerInstance.setLevel(...);
    sdkLoggerInstance.setStream(...);
  }
}

// Phase 2: Lazy Initialization (triggered by first use)
export const sdkLogger: Logger = new Proxy({} as Logger, {
  get(_, prop: string | symbol) {
    const instance = getSDKLoggerInstance(); // ← Triggers initialization
    return instance[prop];
  },
});

function getSDKLoggerInstance(): Logger {
  if (!sdkLoggerInstance) {
    sdkLoggerInstance = initializeSDKLogger(); // ← Uses pendingSDKConfig
  }
  return sdkLoggerInstance;
}

function initializeSDKLogger(config?: {...}): Logger {
  if (isSDKInitialized && sdkLoggerInstance) {
    return sdkLoggerInstance;
  }
  
  // Priority: explicit config > pending config > env vars > default
  const level = config?.level ?? pendingSDKConfig?.level ?? getSDKLogLevel();
  const stream = config?.stream ?? pendingSDKConfig?.stream;
  
  sdkLoggerInstance = createSDKLoggerInstance(level, stream);
  isSDKInitialized = true;
  return sdkLoggerInstance;
}
```

## Race Condition Analysis

### Potential Issue: Early Logger Access

**Scenario**: What if SDK module imports trigger logger access before `configureSDKLogger()` is called?

**Analysis**:
```typescript
// In SDK modules like sdk/core/utils/callback.ts
import { sdkLogger as logger } from "../../utils/logger.js";

// This import does NOT trigger initialization!
// The Proxy is created but not accessed yet.

// Only when logger.error() is called does initialization happen:
logger.error("Error in callback", { error });
// ↑ This triggers Proxy.get() → getSDKLoggerInstance() → initializeSDKLogger()
```

**Conclusion**: ✅ **Safe** - Import statements don't trigger initialization, only method calls do.

### Potential Issue: Asynchronous Bootstrap

**Scenario**: SDK constructor starts async bootstrap immediately. Could it access logger before CLI configures it?

**Analysis**:
```typescript
// sdk/api/shared/core/sdk.ts
class SDK {
  constructor(options?: SDKOptions) {
    this.pendingOptions = options;
    this.factory = new APIFactory(globalContext);
    this.dependencies = new APIDependencyManager();
    
    // Async bootstrap starts immediately
    this.bootstrapPromise = this.bootstrap(options).catch(error => {
      logger.error(`Failed to bootstrap SDK: ${getErrorMessage(error)}`);
      // ↑ This is inside .catch(), won't execute unless bootstrap fails
    });
  }
  
  private async bootstrap(options?: SDKOptions): Promise<void> {
    await options?.hooks?.onBootstrapStart?.();
    
    // ... initialization code ...
    
    logger.info("DI container initialized with storage adapters", {...});
    // ↑ This WILL execute during bootstrap
  }
}
```

**Timeline**:
```
T0: CLI calls initSDKLogger() → sets pendingSDKConfig
T1: CLI calls getSDK() → creates SDK instance
T2: SDK constructor starts async bootstrap
T3: Bootstrap calls logger.info() → Proxy triggers initialization
T4: initializeSDKLogger() uses pendingSDKConfig ✅
```

**Conclusion**: ✅ **Safe** - Even though bootstrap is async, JavaScript's single-threaded nature ensures:
1. `initSDKLogger()` completes fully (sets pendingSDKConfig)
2. Then `getSDK()` is called
3. Bootstrap runs asynchronously but logger access happens after T0

### Potential Issue: Module Load Side Effects

**Scenario**: Do any SDK modules call logger methods during module load (not inside functions)?

**Verification**:
```bash
# Check for top-level logger calls (not inside functions)
grep -r "sdkLogger\." sdk/ --include="*.ts" | grep -v "import" | grep -v "function" | grep -v "//"
```

**Result**: All logger usage is inside functions/methods, not at module top-level.

**Conclusion**: ✅ **Safe** - No module-load side effects.

## Verification Test

To verify the initialization order works correctly, you can add this test:

```typescript
// __tests__/logger-initialization.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { configureSDKLogger, sdkLogger } from '../sdk/utils/logger.js';
import { createConsoleStream } from '@wf-agent/common-utils';

describe('SDK Logger Initialization', () => {
  beforeEach(() => {
    // Reset logger state before each test
    // (You may need to export reset functions for testing)
  });
  
  it('should use configured settings even when accessed after configuration', () => {
    // Configure logger
    configureSDKLogger({
      level: 'debug',
      stream: createConsoleStream({ json: true }),
    });
    
    // Access logger (triggers initialization)
    sdkLogger.info('Test message');
    
    // Verify logger was created with correct config
    // (You may need to export getSDKLoggerInstance for testing)
  });
  
  it('should handle early access gracefully', async () => {
    // Don't configure first - let it use defaults
    const promise = Promise.resolve().then(() => {
      sdkLogger.info('Early access message');
    });
    
    await promise;
    // Should not throw, should use env vars or defaults
  });
});
```

## Conclusion

### Is CLI-APP Control Over SDK Logger Reliable?

**Answer: YES** ✅

The design correctly handles initialization order through:

1. **Pending Configuration Pattern**: Configuration is stored and applied when logger is first created
2. **Proxy-based Lazy Initialization**: Logger instances aren't created until first use
3. **No Module-Load Side Effects**: Imports don't trigger initialization
4. **JavaScript Single-Threaded Execution**: Ensures configuration completes before async bootstrap accesses logger

### Strengths

- ✅ Configuration always applies correctly regardless of access timing
- ✅ Supports runtime reconfiguration via `setLevel()` and `setStream()`
- ✅ Graceful fallback to environment variables if not configured
- ✅ Separate loggers for SDK/Graph/Agent with independent configuration

### Recommendations

1. **Add Explicit Documentation**: Document the initialization order requirement in README
2. **Add Runtime Warning**: If logger is accessed before configuration, log a warning
3. **Add Health Metrics Endpoint**: Expose `getMetrics()` for monitoring
4. **Create Integration Tests**: Test various initialization order scenarios

## Optimizations Applied

1. ✅ Consolidated duplicate `getLogLevelFromEnv()` into common-utils
2. ✅ Added `getMetrics()` method to BaseFileStream for visibility
3. ✅ Fixed rate limiter to use `process.stderr.write()` instead of `console.warn()` to avoid recursion
4. ✅ Added `resetErrorState()` method for error recovery scenarios
