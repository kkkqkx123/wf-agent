# SDK Initialization Guide

This document explains how to properly initialize and use the SDK in your applications.

## Architecture Principles

The SDK follows a clear architectural pattern:

- **SDK provides the mechanism**: Internal initialization logic, preset registration, module preloading
- **Apps provide the policy**: Configuration options, storage adapters, feature toggles
- **Dependency Injection**: Apps implement concrete storage, SDK uses abstract interfaces
- **Sensible Defaults**: Features enabled by default with opt-out capability

## Initialization Flow

When you call `getSDK(options)`, the following happens automatically:

1. **Constructor Phase** (synchronous):
   - Stores configuration options
   - Initializes API factory
   - Creates dependency manager
   - Starts async bootstrap (non-blocking)

2. **Bootstrap Phase** (asynchronous):
   - Initializes storage adapters (if provided)
   - Preloads internal modules (TomlParserManager)
   - Registers presets based on config:
     - Context compression (enabled by default)
     - Predefined tools (enabled by default)
     - Predefined prompts (enabled by default)

3. **Ready State**:
   - All presets registered
   - Storage adapters initialized
   - SDK fully operational

## Basic Usage Patterns

### 1. Simple Initialization (Automatic)

```typescript
import { getSDK } from '@wf-agent/sdk/api';

// Simplest usage - bootstrap happens automatically
const sdk = getSDK({
  debug: false,
  logLevel: "info",
  presets: {
    contextCompression: { enabled: true },
    predefinedTools: { enabled: true },
    predefinedPrompts: { enabled: true },
  },
});

// SDK is initializing in the background
// You can start using it immediately for most operations
```

### 2. Explicit Initialization Control

```typescript
import { getSDK } from '@wf-agent/sdk/api';

const sdk = getSDK({
  debug: true,
  logLevel: "debug",
  presets: {
    predefinedTools: { enabled: true },
  },
});

// Wait for bootstrap to complete
await sdk.waitForReady();

// Now guaranteed that all presets are registered
console.log("SDK is fully ready:", sdk.isReady());
```

**When to use**: Critical paths that depend on fully initialized SDK features.

### 3. Using Lifecycle Hooks

```typescript
import { getSDK, type SDKLifecycleHooks } from '@wf-agent/sdk/api';

const hooks: SDKLifecycleHooks = {
  onBootstrapStart: () => {
    console.log("SDK bootstrap starting...");
    // Start performance timer, show loading indicator, etc.
  },
  onBootstrapComplete: () => {
    console.log("SDK bootstrap completed successfully!");
    // Stop timer, hide loading indicator, emit metrics
  },
  onBootstrapError: (error: Error) => {
    console.error("SDK bootstrap failed:", error);
    // Send error to monitoring service, show user-friendly message
  },
  onDestroy: () => {
    console.log("SDK is being destroyed...");
    // Cleanup app-specific resources
  },
};

const sdk = getSDK({
  presets: { predefinedTools: { enabled: true } },
  hooks,
});

await sdk.waitForReady();
```

**When to use**: Observability, metrics, custom setup/teardown logic.

### 4. Storage Adapter Integration

```typescript
import { getSDK } from '@wf-agent/sdk/api';

// Apps implement storage adapters
const checkpointAdapter = new JsonCheckpointStorage(config);
const workflowAdapter = new SqliteWorkflowStorage(config);
const taskAdapter = new JsonTaskStorage(config);

// Pass adapters to SDK
const sdk = getSDK({
  checkpointStorageAdapter: checkpointAdapter,
  workflowStorageAdapter: workflowAdapter,
  taskStorageAdapter: taskAdapter,
  workflowExecutionStorageAdapter: executionAdapter,
  agentLoopCheckpointStorageAdapter: agentLoopAdapter,
  presets: {
    predefinedTools: { enabled: true },
  },
});

await sdk.waitForReady();

// Storage is now initialized and ready to use
```

**Key Point**: Apps implement concrete storage (JSON, SQLite, etc.), SDK remains agnostic.

### 5. Selective Preset Configuration

```typescript
import { getSDK } from '@wf-agent/sdk/api';

const sdk = getSDK({
  presets: {
    // Enable context compression
    contextCompression: {
      enabled: true,
    },
    
    // Enable only specific tools
    predefinedTools: {
      enabled: true,
      allowList: ["read_file", "write_file"], // Only these tools
      blockList: ["run_shell"], // Except this one
    },
    
    // Disable predefined prompts
    predefinedPrompts: {
      enabled: false,
    },
  },
});

await sdk.waitForReady();
```

### 6. Health Check Pattern

```typescript
import { getSDK } from '@wf-agent/sdk/api';

const sdk = getSDK({
  presets: { predefinedTools: { enabled: true } },
});

// Wait for initialization
await sdk.waitForReady();

// Perform health check
const health = await sdk.healthCheck();
console.log("SDK Health Status:", health.status);
console.log("Module Details:", health.details);

if (health.status === "unhealthy") {
  console.error("Some modules are not working correctly");
}
```

### 7. Graceful Shutdown

```typescript
import { getSDK } from '@wf-agent/sdk/api';

const sdk = getSDK({
  presets: { predefinedTools: { enabled: true } },
});

await sdk.waitForReady();

// ... Use SDK ...

// When done, shutdown gracefully
try {
  await sdk.shutdown(); // Closes storage adapters
  console.log("SDK shut down successfully");
} catch (error) {
  console.error("Error during shutdown:", error);
}

// Or completely destroy (more thorough cleanup)
await sdk.destroy();
console.log("SDK destroyed");
```

### 8. Readiness Check Pattern

```typescript
import { getSDK } from '@wf-agent/sdk/api';

const sdk = getSDK({
  presets: { predefinedTools: { enabled: true } },
});

// Non-blocking check
if (sdk.isReady()) {
  console.log("SDK is already initialized");
  // Safe to use immediately
} else {
  console.log("SDK is still initializing...");
  // Either wait or handle gracefully
  sdk.waitForReady().then(() => {
    console.log("SDK is now ready");
  });
}
```

## Real-World Example: CLI App

Here's how the CLI app initializes the SDK (simplified):

```typescript
import { getSDK } from '@wf-agent/sdk/api';
import { initializeStorageManager, getStorageManager } from './storage';
import { loadConfigWithEnvOverride } from './config';

async function initializeCLI(options: CLIOptions) {
  // 1. Load app configuration
  const config = await loadConfigWithEnvOverride(options.config);

  // 2. Initialize output system
  const output = initializeOutput({
    logFile: options.logFile,
    verbose: options.verbose,
    debug: options.debug,
    outputDir: config.output?.dir,
    enableSDKLogs: config.output?.enableSDKLogs,
    sdkLogLevel: config.output?.sdkLogLevel,
  });

  // 3. Initialize loggers
  initLogger({ /* ... */ });
  initSDKLogger({ /* ... */ });

  // 4. Initialize storage manager
  await initializeStorageManager(config);
  const storageManager = getStorageManager();

  // 5. Initialize SDK with app-provided adapters
  const sdk = getSDK({
    debug: options.debug,
    logLevel: options.debug ? "debug" : options.verbose ? "info" : "warn",
    presets: config.presets,
    checkpointStorageAdapter: storageManager?.getCheckpointStorage(),
    workflowStorageAdapter: storageManager?.getWorkflowStorage(),
    taskStorageAdapter: storageManager?.getTaskStorage(),
    workflowExecutionStorageAdapter: storageManager?.getWorkflowExecutionStorage(),
    agentLoopCheckpointStorageAdapter: storageManager?.getAgentLoopCheckpointStorage(),
    hooks: {
      onBootstrapComplete: () => {
        output.infoLog("CLI SDK initialized successfully");
      },
    },
  });

  // 6. Wait for SDK to be ready
  await sdk.waitForReady();

  // 7. Register app-specific handlers
  const humanRelayHandler = new CLIHumanRelayHandler();
  sdk.humanRelay.registerHandler(humanRelayHandler);

  return sdk;
}
```

## Best Practices

### ✅ DO:

1. **Pass configuration through SDKOptions**
   - Let SDK handle internal initialization
   - Control behavior through options, not manual calls

2. **Use `waitForReady()` when timing matters**
   - For critical paths that need full initialization
   - Before accessing preset-dependent features

3. **Implement lifecycle hooks for observability**
   - Track initialization performance
   - Handle errors gracefully
   - Cleanup app-specific resources

4. **Provide storage adapters via dependency injection**
   - Apps implement concrete storage (JSON, SQLite, etc.)
   - SDK remains agnostic to implementation details

5. **Handle errors gracefully**
   - Use try-catch around `waitForReady()` if needed
   - Check health status after initialization

### ❌ DON'T:

1. **Try to manually initialize SDK internals**
   - Don't call `bootstrap()` directly
   - Don't access private methods or properties

2. **Duplicate initialization logic across apps**
   - Each app should follow the same pattern
   - Centralize common initialization in shared utilities

3. **Ignore initialization errors**
   - Always check for errors in production code
   - Use lifecycle hooks to monitor failures

4. **Assume immediate readiness**
   - Bootstrap is asynchronous
   - Use `isReady()` or `waitForReady()` to check status

## Configuration Reference

### SDKOptions

```typescript
interface SDKOptions {
  // Debug mode
  debug?: boolean;
  
  // Log level
  logLevel?: "debug" | "info" | "warn" | "error";
  
  // Default timeout (milliseconds)
  defaultTimeout?: number;
  
  // Enable checkpoints
  enableCheckpoints?: boolean;
  
  // Storage adapters (implemented by apps)
  checkpointStorageAdapter?: CheckpointStorageAdapter;
  workflowStorageAdapter?: WorkflowStorageAdapter;
  taskStorageAdapter?: TaskStorageAdapter;
  workflowExecutionStorageAdapter?: WorkflowExecutionStorageAdapter;
  agentLoopCheckpointStorageAdapter?: AgentLoopCheckpointStorageAdapter;
  
  // Enable validation
  enableValidation?: boolean;
  
  // Preset configuration
  presets?: PresetsConfig;
  
  // Lifecycle hooks
  hooks?: SDKLifecycleHooks;
}
```

### SDKLifecycleHooks

```typescript
interface SDKLifecycleHooks {
  // Called when bootstrap starts
  onBootstrapStart?: () => void | Promise<void>;
  
  // Called when bootstrap completes successfully
  onBootstrapComplete?: () => void | Promise<void>;
  
  // Called when bootstrap fails
  onBootstrapError?: (error: Error) => void | Promise<void>;
  
  // Called when SDK is being destroyed
  onDestroy?: () => void | Promise<void>;
}
```

## Troubleshooting

### SDK not ready when I try to use it

**Problem**: Calling SDK methods before bootstrap completes.

**Solution**: Use `waitForReady()`:
```typescript
const sdk = getSDK(options);
await sdk.waitForReady();
// Now safe to use
```

### Presets not registered

**Problem**: Expected tools/workflows not available.

**Solution**: 
1. Check preset configuration in options
2. Verify `waitForReady()` was called
3. Check logs for bootstrap errors

### Storage adapter not working

**Problem**: Data not persisting.

**Solution**:
1. Ensure adapter is passed in options
2. Check adapter initialization in app layer
3. Verify adapter implements correct interface

### Bootstrap errors

**Problem**: SDK fails to initialize.

**Solution**:
1. Use `onBootstrapError` hook to capture errors
2. Check logs for detailed error messages
3. Verify storage adapters are properly configured

## Summary

The SDK initialization design follows the principle: **"SDK provides the mechanism, apps provide the policy"**. 

- Apps control WHAT gets initialized through configuration
- SDK handles HOW initialization happens internally
- This separation keeps apps simple and SDK flexible

For most use cases, simple initialization is sufficient:
```typescript
const sdk = getSDK({ presets: { predefinedTools: { enabled: true } } });
await sdk.waitForReady();
```

For advanced scenarios, use lifecycle hooks and explicit readiness checks to gain more control over the initialization process.
