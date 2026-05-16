# Timeout Management System

Unified timeout management for the wf-agent SDK.

## Overview

The timeout management system provides a centralized, consistent approach to handling timeouts across all modules in the SDK. It replaces fragmented timeout implementations with a unified API that integrates with the interruption system and provides comprehensive observability.

## Features

- **Unified Interface**: Single API for all timeout operations
- **Interruption Integration**: Automatic binding to InterruptionState
- **Resource Safety**: Guaranteed cleanup and no memory leaks
- **Observability**: Comprehensive metrics and logging
- **Flexibility**: Support multiple timeout strategies (absolute, idle, hierarchical)
- **Checkpoint Support**: Serializable state for persistence

## Quick Start

### Basic Usage

```typescript
import { TimeoutRegistry } from "@wf-agent/sdk/core";

// Get or create registry
const registry = new TimeoutRegistry();

// Register a timeout
const handle = registry.register(executionId, {
  id: 'my-timeout',
  duration: 30000, // 30 seconds
  onTimeout: () => {
    console.log('Timeout expired!');
  },
});

// Cancel if operation completes early
handle.cancel();

// Check remaining time
const remaining = handle.getRemainingTime();
```

### With Interruption State

```typescript
const handle = registry.register(executionId, {
  id: 'llm-call',
  duration: 60000,
  onTimeout: () => cancelLLMCall(),
  interruptionState: executionEntity.getInterruptionState(),
  tag: 'llm',
});
```

### With Warning

```typescript
const handle = registry.register(executionId, {
  id: 'long-operation',
  duration: 300000, // 5 minutes
  warningThreshold: 60000, // Warn at 4 minutes
  onWarning: () => sendAlert('Operation taking long'),
  onTimeout: () => cancelOperation(),
});
```

## Advanced Strategies

### Idle Timeout

Triggers after a period of inactivity:

```typescript
import { registerIdleTimeout } from "@wf-agent/sdk/core/utils/timeout";

const manager = registry.getManager(executionId);

const handle = registerIdleTimeout(manager, {
  id: 'idle-check',
  idleDuration: 60000,
  activityDetector: () => isStillProcessing(),
  onTimeout: () => cleanup(),
});
```

### Hierarchical Timeout

Parent-child timeout relationships:

```typescript
import { registerHierarchicalTimeout } from "@wf-agent/sdk/core/utils/timeout";

const child1 = manager.register({ id: 'child1', duration: 5000, ... });
const child2 = manager.register({ id: 'child2', duration: 5000, ... });

const parent = registerHierarchicalTimeout(manager, {
  id: 'parent',
  duration: 15000,
  children: [child1, child2],
  onChildTimeout: (childId) => console.log(`${childId} timed out`),
  onTimeout: () => handleParentTimeout(),
});
```

### Retry with Adaptive Timeout

```typescript
import { retryWithTimeout } from "@wf-agent/sdk/core/utils/timeout";

const result = await retryWithTimeout({
  fn: () => fetchData(),
  maxRetries: 3,
  baseTimeout: 5000,
  maxTimeout: 30000,
  retryDelay: 1000,
  onRetry: (attempt, error) => {
    console.log(`Retry ${attempt}: ${error.message}`);
  },
});
```

## Utility Functions

### Combine Timeout with AbortSignal

```typescript
import { combineTimeoutWithSignal } from "@wf-agent/sdk/core/utils/timeout";

const { signal, clearTimeout } = combineTimeoutWithSignal(5000, existingSignal);

try {
  await fetch(url, { signal });
} finally {
  clearTimeout();
}
```

### Create Timeout Promise

```typescript
import { createTimeoutPromise } from "@wf-agent/sdk/core/utils/timeout";

const result = await createTimeoutPromise(
  fetchData(),
  5000,
  'Data fetch timed out'
);
```

### Execute with Timeout

```typescript
import { withTimeout } from "@wf-agent/sdk/core/utils/timeout";

const result = await withTimeout(
  () => fetchData(),
  5000,
  { 
    onTimeout: () => console.log('Timed out'),
    message: 'Custom timeout message'
  }
);
```

## Configuration

### TimeoutManager Config

```typescript
import { TimeoutManager } from "@wf-agent/sdk/core";

const manager = new TimeoutManager({
  defaultTimeout: 30000,           // 30 seconds
  maxTimeout: 86400000,            // 24 hours
  enableWarnings: true,
  defaultWarningThreshold: 60000,  // 1 minute
  enableMetrics: true,
});
```

### TimeoutRegistry Config

```typescript
import { TimeoutRegistry } from "@wf-agent/sdk/core";

const registry = new TimeoutRegistry({
  defaultManagerConfig: {
    defaultTimeout: 30000,
    maxTimeout: 86400000,
  },
  autoCleanup: true,
  metricsInterval: 60000,
  maxTimeoutsPerExecution: 1000,
});
```

## Observability

### Get Statistics

```typescript
// Per-manager stats
const manager = registry.getManager(executionId);
const stats = manager.getStats();

console.log(stats.activeTimeouts);
console.log(stats.totalRegistered);
console.log(stats.timedOutCount);
console.log(stats.byTag);

// Global stats
const globalStats = registry.getStats();
console.log(globalStats.activeExecutions);
console.log(globalStats.totalTimeouts);
```

## Lifecycle Management

### Cleanup on Execution End

```typescript
// Automatically cancels all timeouts for an execution
registry.cleanup(executionId);

// Or cleanup all executions (e.g., on shutdown)
registry.cleanupAll();
```

### Checkpoint Support

```typescript
// Serialize timeout state
const snapshot = manager.serialize();

// Restore timeout state (timers not restored, only metadata)
manager.restore(snapshot);
```

## Migration Guide

### From setTimeout

**Before:**
```typescript
const timerId = setTimeout(() => {
  cancelOperation();
}, 30000);

// Later...
clearTimeout(timerId);
```

**After:**
```typescript
const handle = registry.register(executionId, {
  id: 'operation',
  duration: 30000,
  onTimeout: () => cancelOperation(),
});

// Later...
handle.cancel();
```

### From Promise.race

**Before:**
```typescript
const timeout = new Promise((_, reject) => {
  setTimeout(() => reject(new Error('Timeout')), 5000);
});

await Promise.race([operation(), timeout]);
```

**After:**
```typescript
import { createTimeoutPromise } from "@wf-agent/sdk/core/utils/timeout";

await createTimeoutPromise(operation(), 5000);
```

## Best Practices

1. **Always use unique IDs**: Ensure each timeout has a unique ID within an execution
2. **Cancel on completion**: Always cancel timeouts when operations complete successfully
3. **Use tags for grouping**: Tag timeouts by module (e.g., 'llm', 'tool', 'join')
4. **Bind to interruption state**: Always provide interruptionState when available
5. **Set reasonable timeouts**: Use adaptive timeouts for retries
6. **Monitor statistics**: Regularly check timeout stats for anomalies
7. **Clean up properly**: Always call `registry.cleanup()` when executions end

## Architecture

See [timeout-management.md](../../../docs/architecture/timeout-management.md) for detailed architecture documentation.

## API Reference

### TimeoutManager

- `register(options)`: Register a new timeout
- `cancel(handle)`: Cancel a timeout
- `refresh(handle)`: Reset timeout timer
- `getRemainingTime(handle)`: Get remaining time
- `getStats()`: Get statistics
- `size()`: Get active timeout count
- `isEmpty()`: Check if empty
- `clear()`: Cancel all timeouts
- `serialize()`: Serialize state
- `restore(snapshot)`: Restore state

### TimeoutRegistry

- `getManager(executionId)`: Get/create manager
- `register(executionId, options)`: Register timeout
- `cancelByExecutionId(executionId)`: Cancel all for execution
- `getStats()`: Get global statistics
- `cleanup(executionId)`: Clean up execution
- `cleanupAll()`: Clean up all

### Utility Functions

- `combineTimeoutWithSignal(duration, signal)`
- `createTimeoutPromise(promise, duration, message)`
- `calculateAdaptiveTimeout(base, retryCount, max)`
- `delay(ms, signal)`
- `withTimeout(fn, duration, options)`
- `isTimeoutError(error)`

### Strategies

- `registerIdleTimeout(manager, options)`
- `registerHierarchicalTimeout(manager, options)`
- `registerTwoStageTimeout(manager, options)`
- `retryWithTimeout(options)`
