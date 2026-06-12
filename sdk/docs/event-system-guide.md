# Event System Guide

## Overview

The event system provides a flexible mechanism for monitoring and reacting to workflow execution events. It supports two types of listeners:

1. **Global Listeners**: Receive events from all executions
2. **Execution-scoped Listeners**: Only receive events for a specific execution (auto-cleanup)

## Quick Start

### Global Listener

```typescript
const sdk = createSDK(options);
await sdk.waitForReady();

// Listen to all NODE_COMPLETED events across all executions
sdk.events.on("NODE_COMPLETED", event => {
  console.log("Node completed:", event.nodeId, "in execution:", event.executionId);
});
```

### Execution-scoped Listener

```typescript
// Listen only to events from a specific execution
const executionId = "exec-123";

sdk.events.on(
  "NODE_COMPLETED",
  event => {
    console.log("Node completed in this execution:", event.nodeId);
  },
  {
    executionId, // This makes it execution-scoped
    priority: 10, // Optional: higher priority listeners execute first
  },
);

// Listeners are automatically cleaned up when execution ends
```

## API Reference

### EventResourceAPI Methods

#### Subscription Methods

```typescript
// Register global listener
on(eventType: EventType, listener: EventListener, options?: {
  priority?: number;
  filter?: (event) => boolean;
  timeout?: number;
}): () => void;

// Register one-time listener
once(eventType: EventType, listener: EventListener): () => void;

// Unregister listener
off(eventType: EventType, listener: EventListener): boolean;

// Wait for specific event
waitFor<T extends BaseEvent>(
  eventType: EventType,
  timeout?: number,
  filter?: (event: T) => boolean
): Promise<T>;
```

#### Diagnostic Methods

```typescript
// Get execution-scoped listener statistics
getExecutionListenerStats(): Map<string, number>;

// Get detailed information about all active listeners
getAllListenerInfo(): Array<{
  id: string;
  eventType: string;
  executionId?: string;
  priority: number;
  registeredAt: number;
  metrics?: {
    totalExecutions: number;
    averageDuration: number;
    failureCount: number;
    slowExecutionCount: number;
  };
}>;

// Get listener counts by event type
getListenerCountByEventType(): Map<string, {
  total: number;
  executionScoped: number;
  global: number;
}>;

// Get comprehensive event system health overview
getEventSystemHealth(): {
  totalListeners: number;
  activeExecutionCount: number;
  historySize: number;
  listenerDistribution: Map<string, { total: number; executionScoped: number; global: number }>;
  executionStats: Map<string, number>;
};
```

## Best Practices

### 1. Use Execution-scoped Listeners for Execution-specific Logic

```typescript
// ✅ Good: Automatically cleaned up when execution ends
sdk.events.on("NODE_COMPLETED", handler, { executionId });

// ❌ Bad: Manual cleanup required, risk of memory leak
sdk.events.on("NODE_COMPLETED", handler);
```

### 2. Use Filters to Reduce Noise

```typescript
// Only listen to specific nodes
sdk.events.on(
  "NODE_COMPLETED",
  event => {
    // Handle event
  },
  {
    filter: event => event.nodeId === "important-node",
  },
);
```

### 3. Monitor Listener Health

```typescript
// Check for potential memory leaks
const health = sdk.events.getEventSystemHealth();
if (health.activeExecutionCount > 100) {
  console.warn("Many executions with active listeners - check for cleanup issues");
}

// Find slow listeners
const listeners = sdk.events.getAllListenerInfo();
const slowListeners = listeners.filter(l => l.metrics?.slowExecutionCount > 10);
```

### 4. Set Timeouts for Long-running Listeners

```typescript
// Prevent listeners from blocking execution
sdk.events.on(
  "NODE_COMPLETED",
  async event => {
    // This will timeout after 5 seconds
    await doSomethingSlow();
  },
  {
    timeout: 5000, // 5 seconds
  },
);
```

## Architecture

### Instance Isolation

Each SDK instance has its own isolated EventRegistry:

```typescript
const sdk1 = createSDK({
  /* config 1 */
});
const sdk2 = createSDK({
  /* config 2 */
});

// These are completely independent
sdk1.events.on("NODE_COMPLETED", listener1);
sdk2.events.on("NODE_COMPLETED", listener2);

// listener1 only receives events from sdk1
// listener2 only receives events from sdk2
```

### Lifecycle Management

Execution-scoped listeners are automatically cleaned up when:

- Workflow execution completes successfully
- Workflow execution fails
- Workflow execution is cancelled

The cleanup happens in `WorkflowLifecycleCoordinator.stopWorkflowExecution()`.

## Debugging

### Finding Active Listeners

```typescript
// See all active listeners
const listeners = sdk.events.getAllListenerInfo();
console.table(
  listeners.map(l => ({
    eventType: l.eventType,
    executionId: l.executionId || "global",
    priority: l.priority,
    executions: l.metrics?.totalExecutions || 0,
  })),
);
```

### Detecting Memory Leaks

```typescript
// Check for executions with too many listeners
const stats = sdk.events.getExecutionListenerStats();
for (const [executionId, count] of stats) {
  if (count > 50) {
    console.warn(`Execution ${executionId} has ${count} listeners - possible leak`);
  }
}
```

### Monitoring Performance

```typescript
// Find listeners with high failure rates
const listeners = sdk.events.getAllListenerInfo();
const problematicListeners = listeners.filter(
  l => l.metrics && l.metrics.failureCount > 5 && l.metrics.totalExecutions > 10,
);

problematicListeners.forEach(l => {
  console.error(`Listener ${l.id} has ${l.metrics.failureCount} failures`);
});
```

## Common Patterns

### Pattern 1: Progress Tracking

```typescript
function trackExecutionProgress(executionId: string) {
  let completedNodes = 0;
  let totalNodes = 0;

  const unsubscribe = sdk.events.on(
    "NODE_COMPLETED",
    event => {
      if (event.executionId === executionId) {
        completedNodes++;
        console.log(`Progress: ${completedNodes}/${totalNodes}`);
      }
    },
    { executionId },
  );

  return unsubscribe; // Can manually cleanup if needed
}
```

### Pattern 2: Error Aggregation

```typescript
function aggregateErrors(executionId: string) {
  const errors: Array<{ nodeId: string; error: string }> = [];

  sdk.events.on(
    "NODE_FAILED",
    event => {
      if (event.executionId === executionId) {
        errors.push({
          nodeId: event.nodeId,
          error: event.error.message,
        });
      }
    },
    { executionId },
  );

  return {
    getErrors: () => [...errors],
    clear: () => {
      errors.length = 0;
    },
  };
}
```

### Pattern 3: Cross-execution Analytics

```typescript
// Global listener for analytics
const analytics = {
  totalExecutions: 0,
  totalNodes: 0,
  avgExecutionTime: 0,
};

sdk.events.on("WORKFLOW_EXECUTION_COMPLETED", event => {
  analytics.totalExecutions++;
  analytics.totalNodes += event.nodeCount || 0;
  // Update averages, etc.
});
```

## Migration Guide

### From Old Documentation

If you were using the old event system analysis document, here's what changed:

1. **Clarified terminology**: "Global" vs "Execution-scoped" instead of ambiguous "instance-level"
2. **Added diagnostic APIs**: Now you can monitor listener health and detect issues
3. **Improved documentation**: Clear examples and best practices
4. **Removed outdated analysis**: The old `event-system-analysis.md` has been replaced with this practical guide

### Key Changes

- Execution-scoped listeners now have better validation
- Added `cleanupExecutionListeners()` parameter validation
- New diagnostic methods for debugging
- Improved comments and examples throughout the codebase
