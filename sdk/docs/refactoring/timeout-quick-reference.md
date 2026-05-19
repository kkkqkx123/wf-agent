# Timeout Management Quick Reference

## Standard Tags

### LLM Timeouts
```typescript
import { LLM_TIMEOUT_TAGS } from '@wf-agent/sdk/core/types';

LLM_TIMEOUT_TAGS.CALL    // 'llm-call' - Single LLM call
LLM_TIMEOUT_TAGS.STREAM  // 'llm-stream' - LLM streaming response
LLM_TIMEOUT_TAGS.RETRY   // 'llm-retry' - LLM retry attempt
```

### Tool Timeouts
```typescript
import { TOOL_TIMEOUT_TAGS } from '@wf-agent/sdk/core/types';

TOOL_TIMEOUT_TAGS.EXECUTION  // 'tool-execution' - Generic tool execution
TOOL_TIMEOUT_TAGS.SHELL      // 'tool-shell' - Shell command execution
TOOL_TIMEOUT_TAGS.API        // 'tool-api' - API call tool
```

### Workflow Timeouts
```typescript
import { WORKFLOW_TIMEOUT_TAGS } from '@wf-agent/sdk/core/types';

WORKFLOW_TIMEOUT_TAGS.EXECUTION  // 'workflow-execution' - Overall workflow
WORKFLOW_TIMEOUT_TAGS.PAUSE      // 'workflow-pause' - Pause monitoring
WORKFLOW_TIMEOUT_TAGS.NODE       // 'workflow-node' - Node execution
```

### Interruption Timeouts
```typescript
import { INTERRUPTION_TIMEOUT_TAGS } from '@wf-agent/sdk/core/types';

INTERRUPTION_TIMEOUT_TAGS.HOOK    // 'interruption-hook' - Hook execution
INTERRUPTION_TIMEOUT_TAGS.CLEANUP // 'interruption-cleanup' - Cleanup ops
```

### User Interaction Timeouts
```typescript
import { USER_TIMEOUT_TAGS } from '@wf-agent/sdk/core/types';

USER_TIMEOUT_TAGS.INPUT    // 'user-input' - Waiting for input
USER_TIMEOUT_TAGS.APPROVAL // 'user-approval' - Waiting for approval
```

## Basic Usage

### Register a Timeout
```typescript
import { TimeoutManager, LLM_TIMEOUT_TAGS } from '@wf-agent/sdk/core';

const manager = new TimeoutManager();

const handle = manager.register({
  id: 'unique-timeout-id',
  duration: 30000, // 30 seconds
  tag: LLM_TIMEOUT_TAGS.CALL,
  onTimeout: async () => {
    console.log('Timeout expired!');
  },
  onWarning: async () => {
    console.log('Warning: Running low on time');
  },
  warningThreshold: 5000, // Warn 5 seconds before timeout
  metadata: { nodeId: 'agent-node-1' }
});
```

### Manage Timeout
```typescript
// Check if active
if (handle.isActive()) {
  console.log('Still running');
}

// Get remaining time
const remaining = handle.getRemainingTime();
console.log(`${remaining}ms remaining`);

// Cancel timeout
handle.cancel();

// Refresh timeout (reset timer)
manager.refresh(handle);
```

### With Interruption Protection
```typescript
const handle = manager.registerWithInterruptionProtection(
  {
    id: 'protected-operation',
    duration: 60000,
    tag: WORKFLOW_TIMEOUT_TAGS.NODE,
    onTimeout: cleanup
  },
  interruptionState // Automatically cancelled on interruption
);
```

## TimeoutRegistry Usage

### Register Across Executions
```typescript
import { TimeoutRegistry, TOOL_TIMEOUT_TAGS } from '@wf-agent/sdk/core';

const registry = new TimeoutRegistry();

// Register in different executions
registry.register('execution-1', {
  id: 'tool-op-1',
  duration: 10000,
  tag: TOOL_TIMEOUT_TAGS.EXECUTION,
  onTimeout: async () => {}
});

registry.register('execution-2', {
  id: 'tool-op-2',
  duration: 10000,
  tag: TOOL_TIMEOUT_TAGS.EXECUTION,
  onTimeout: async () => {}
});
```

### Batch Operations
```typescript
// Cancel all timeouts with a specific tag
registry.cancelByTag(TOOL_TIMEOUT_TAGS.EXECUTION);

// Cancel all timeouts for an execution
registry.cancelByExecutionId('execution-1');

// Get global statistics
const stats = registry.getStats();
console.log(stats.byTag);      // Count by tag
console.log(stats.byCategory); // Count by category (llm, tool, etc.)
```

## Utility Functions

### Create Timeout Error
```typescript
import { createTimeoutError } from '@wf-agent/sdk/core/utils/timeout';

const error = createTimeoutError(
  'llm-call-001',
  5000,      // configured duration
  5023,      // actual duration
  'llm-call' // tag
);
// Error: Timeout 'llm-call-001' expired after 5023ms (configured: 5000ms, tag: llm-call)
```

### Execute with Shared Timeout
```typescript
import { executeWithSharedTimeout } from '@wf-agent/sdk/core/utils/timeout';

const results = await executeWithSharedTimeout(
  {
    fetch: () => fetchData(),
    process: () => processData(),
    save: () => saveData()
  },
  10000, // All must complete within 10s
  {
    onTimeout: () => console.log('Timed out'),
    message: 'Batch operation timeout'
  }
);

console.log(results.get('fetch'));
```

### Other Utilities
```typescript
import {
  withTimeout,
  createTimeoutPromise,
  isTimeoutError,
  delay,
  combineTimeoutWithSignal,
  calculateAdaptiveTimeout
} from '@wf-agent/sdk/core/utils/timeout';

// Execute function with timeout
const result = await withTimeout(
  () => fetchData(),
  5000,
  { onTimeout: () => console.log('Timed out') }
);

// Wrap existing promise with timeout
const data = await createTimeoutPromise(fetchData(), 5000);

// Check if error is timeout
if (isTimeoutError(error)) {
  console.log('Operation timed out');
}

// Delay with abort support
await delay(1000, abortSignal);

// Adaptive timeout for retries
const timeout = calculateAdaptiveTimeout(5000, retryCount, 30000);
```

## Validation

### Validate Tags
```typescript
import { isValidTimeoutTag, getTagCategory } from '@wf-agent/sdk/core/types';

// Check if tag is valid
if (isValidTimeoutTag('llm-call')) {
  console.log('Valid standard tag');
}

// Get category from tag
const category = getTagCategory('llm-call');
console.log(category); // 'llm'
```

## Statistics

### Manager Statistics
```typescript
const stats = manager.getStats();

console.log(stats.activeTimeouts);    // Currently active
console.log(stats.totalRegistered);   // Lifetime total
console.log(stats.timedOutCount);     // Expired naturally
console.log(stats.cancelledCount);    // Cancelled manually
console.log(stats.averageDuration);   // Average duration
console.log(stats.byTag);             // Count by tag
```

### Registry Statistics
```typescript
const stats = registry.getStats();

console.log(stats.activeExecutions);  // Active execution count
console.log(stats.totalTimeouts);     // Total active timeouts
console.log(stats.byTag);             // { 'llm-call': 5, 'tool-execution': 3 }
console.log(stats.byCategory);        // { 'llm': 5, 'tool': 3 }
```

## Best Practices

### ✅ DO
- Use standard tags for consistency
- Provide meaningful timeout IDs
- Set appropriate warning thresholds
- Use `registerWithInterruptionProtection()` when applicable
- Monitor statistics for debugging
- Clean up timeouts when done

### ❌ DON'T
- Use arbitrary tag names (use standard tags)
- Set warning threshold >= duration
- Forget to cancel unnecessary timeouts
- Ignore timeout warnings
- Create too many concurrent timeouts (max 1000 per execution)

## Common Patterns

### Pattern 1: LLM Call with Retry
```typescript
async function callLLMWithRetry(maxRetries: number) {
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    const timeout = calculateAdaptiveTimeout(5000, attempt, 30000);
    
    try {
      return await withTimeout(
        () => makeLLMCall(),
        timeout,
        { tag: LLM_TIMEOUT_TAGS.RETRY }
      );
    } catch (error) {
      if (attempt === maxRetries) throw error;
      await delay(1000); // Wait before retry
    }
  }
}
```

### Pattern 2: Workflow Node Execution
```typescript
async function executeNode(nodeId: string, interruptionState: InterruptionState) {
  const handle = manager.registerWithInterruptionProtection(
    {
      id: `node-${nodeId}`,
      duration: 60000,
      tag: WORKFLOW_TIMEOUT_TAGS.NODE,
      warningThreshold: 10000,
      onWarning: () => logWarning(`Node ${nodeId} taking long`),
      onTimeout: () => cancelNode(nodeId),
      metadata: { nodeId }
    },
    interruptionState
  );
  
  try {
    return await executeNodeLogic(nodeId);
  } finally {
    handle.cancel(); // Clean up
  }
}
```

### Pattern 3: Batch Operations
```typescript
async function processBatch(items: any[]) {
  const results = await executeWithSharedTimeout(
    Object.fromEntries(
      items.map((item, idx) => [
        `item-${idx}`,
        () => processItem(item)
      ])
    ),
    30000, // 30s for entire batch
    {
      onTimeout: () => logTimeout('Batch processing'),
      message: 'Batch processing timeout'
    }
  );
  
  return results;
}
```

## Troubleshooting

### Timeout Not Firing
- Check if timeout was cancelled
- Verify duration is positive
- Ensure event loop is not blocked

### Too Many Warnings
- Adjust warning threshold
- Check if timeout duration is appropriate
- Consider increasing timeout

### Memory Leaks
- Always cancel timeouts when done
- Use TimeoutRegistry for automatic cleanup
- Monitor `activeTimeouts` count

### Tag Validation Warnings
- Use standard tags from timeout-tags.ts
- Custom tags should follow pattern: `{prefix}-{suffix}`
- Valid prefixes: llm, tool, workflow, interruption, user

---

For more details, see:
- [Phase 1 Improvements](./phase1-improvements.md)
- [Phase 1 Completion Summary](./phase1-completion-summary.md)
- [Timeout Unification Plan](./timeout-unification-refactoring-plan.md)
