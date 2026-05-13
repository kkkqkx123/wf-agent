# Event System Refactoring Plan

## Overview

This document outlines the refactoring plan for the event system to enforce execution-scoped listeners as the default behavior and eliminate global listeners.

**Design Principle**: Skip backward compatibility phase and directly implement the new architecture.

---

## Current Problems

### 1. Memory Leak Risk
Global listeners are never automatically cleaned up, leading to potential memory leaks in long-running applications.

```typescript
// ❌ Current problematic pattern
sdk.events.on('NODE_COMPLETED', (event) => {
  console.log(event.nodeId);
});
// This listener persists forever unless manually removed
```

### 2. Lack of Isolation
All executions within an SDK instance share the same EventRegistry, causing:
- Cross-execution event interference
- No tenant isolation
- Difficulty in debugging execution-specific issues

### 3. Inconsistent Usage
Internal coordinators use global listeners but rely on `executionId` filtering:
```typescript
// sdk/core/coordinators/followup-question-coordinator.ts
this.eventManager.on("FOLLOWUP_QUESTION_REQUESTED", handler);
// Handler must check event.executionId to filter relevant events
```

### 4. waitFor Already Uses Filtering
The `waitFor` utility already implements executionId filtering, proving the pattern is needed:
```typescript
// sdk/workflow/execution/utils/event/event-waiter.ts
eventManager.waitFor(eventType, timeout, event => event.executionId === executionId)
```

---

## Target Architecture

### Execution-Scoped Listeners as Default

All event listeners must be associated with a specific execution context:

```typescript
// ✅ New required pattern
const executionId = await sdk.startWorkflow(workflowId, options);

sdk.events.on('NODE_COMPLETED', (event) => {
  console.log('Node completed:', event.nodeId);
}, { executionId }); // Required parameter

// Automatic cleanup when execution ends
```

### Global Listeners Removed

- No more global listeners by default
- Monitoring/logging should use alternative mechanisms (structured logging, metrics export)
- Debugging tools can access event history through diagnostic APIs

---

## Implementation Plan

### Phase 1: Core Changes (Week 1-2)

#### 1.1 Modify EventRegistry API

**File**: `sdk/core/registry/event-registry.ts`

**Changes**:
```typescript
interface OnOptions {
  priority?: number;
  filter?: (event: T) => boolean;
  timeout?: number;
  executionId: string; // Now REQUIRED
}

on<T extends BaseEvent>(
  eventType: EventType,
  listener: EventListener<T>,
  options: OnOptions, // executionId is mandatory
): () => void;

once<T extends BaseEvent>(
  eventType: EventType,
  listener: EventListener<T>,
  options: OnOptions, // executionId is mandatory
): () => void;
```

**Remove methods**:
- Remove ability to register listeners without `executionId`
- Throw `RuntimeValidationError` if `executionId` is missing

#### 1.2 Update Subscription Pattern

**File**: `sdk/api/shared/types/subscription.ts`

**Changes**:
```typescript
export class OnEventSubscription<T extends BaseEvent = BaseEvent> extends BaseSubscription<T> {
  constructor(
    private readonly eventType: EventType,
    private readonly listener: EventListener<T>,
    private readonly eventManager: EventRegistry,
    private readonly options: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string; // Required
    },
  ) {
    super();
    
    // Validate executionId is provided
    if (!options.executionId) {
      throw new RuntimeValidationError(
        "executionId is required for event subscriptions",
        { field: "options.executionId" }
      );
    }
  }
  
  subscribe(): () => void {
    return this.eventManager.on(this.eventType, this.listener, this.options);
  }
}
```

#### 1.3 Update APIDependencyManager Helper Functions

**File**: `sdk/api/workflow/operations/events/on-event-subscription.ts`

**Changes**:
```typescript
export function createExecutionScopedSubscription(
  executionId: string,
  eventType: EventType,
  listener: EventListener<BaseEvent>,
  dependencies: APIDependencyManager,
  additionalOptions?: Omit<OnEventParams['options'], 'executionId'>,
): OnEventSubscription {
  return new OnEventSubscription(
    {
      eventType,
      listener,
      options: {
        ...additionalOptions,
        executionId, // Always inject executionId
      },
    },
    dependencies,
  );
}

// Remove or deprecate any functions that don't require executionId
```

---

### Phase 2: Internal Coordinator Refactoring (Week 2-3)

#### 2.1 FollowupQuestionCoordinator

**File**: `sdk/core/coordinators/followup-question-coordinator.ts`

**Before**:
```typescript
public initialize(): void {
  this.unsubscribe = this.eventManager.on(
    "FOLLOWUP_QUESTION_REQUESTED",
    this.handleFollowupQuestionRequest.bind(this),
  );
}

private async handleFollowupQuestionRequest(event: any): Promise<void> {
  const executionId = event.executionId;
  // Process request...
}
```

**After**:
```typescript
private activeRequests: Map<string, {
  unsubscribe: () => void;
  timeoutId?: NodeJS.Timeout;
}> = new Map();

public async handleFollowupQuestionRequest(executionId: string, requestData: FollowupQuestionRequestData): Promise<FollowupQuestionResponseData> {
  return new Promise((resolve, reject) => {
    // Register response listener scoped to this execution
    const unsubscribe = this.eventManager.on(
      "FOLLOWUP_QUESTION_RESPONDED",
      (event: FollowupQuestionRespondedEvent) => {
        if (event.executionId === executionId) {
          this.cleanupRequest(executionId);
          resolve(event.data);
        }
      },
      { executionId } // Execution-scoped listener
    );
    
    // Set timeout
    const timeoutId = setTimeout(() => {
      this.cleanupRequest(executionId);
      reject(new Error("Follow-up question timed out"));
    }, this.timeoutMs);
    
    this.activeRequests.set(executionId, { unsubscribe, timeoutId });
    
    // Emit request event
    this.emitFollowupQuestionRequested(executionId, requestData);
  });
}

private cleanupRequest(executionId: string): void {
  const request = this.activeRequests.get(executionId);
  if (request) {
    request.unsubscribe();
    if (request.timeoutId) clearTimeout(request.timeoutId);
    this.activeRequests.delete(executionId);
  }
}

// Remove initialize() method - no longer needs global listener
```

#### 2.2 LLMExecutionCoordinator - Tool Approval

**File**: `sdk/workflow/execution/coordinators/llm-execution-coordinator.ts`

**Before**:
```typescript
private async waitForToolApproval(toolCallId: string, timeoutMs: number): Promise<ToolApprovalResponse> {
  return new Promise((resolve, reject) => {
    let timeoutId: NodeJS.Timeout | undefined;
    
    const handler = (event: ToolApprovalRespondedEvent) => {
      if (event.toolCallId === toolCallId) {
        if (timeoutId) clearTimeout(timeoutId);
        eventManager.off("TOOL_APPROVAL_RESPONDED", handler);
        resolve(event.data);
      }
    };
    
    if (timeoutMs > 0) {
      timeoutId = setTimeout(() => {
        eventManager.off("TOOL_APPROVAL_RESPONDED", handler);
        reject(new Error(`User interaction timeout after ${timeoutMs}ms`));
      }, timeoutMs);
    }
    
    eventManager.on("TOOL_APPROVAL_RESPONDED", handler);
  });
}
```

**After**:
```typescript
private async waitForToolApproval(
  executionId: string,
  toolCallId: string, 
  timeoutMs: number
): Promise<ToolApprovalResponse> {
  return new Promise((resolve, reject) => {
    let timeoutId: NodeJS.Timeout | undefined;
    
    const handler = (event: ToolApprovalRespondedEvent) => {
      if (event.toolCallId === toolCallId) {
        if (timeoutId) clearTimeout(timeoutId);
        resolve(event.data);
      }
    };
    
    // Use execution-scoped listener - auto cleanup on execution end
    const unsubscribe = this.eventManager.on(
      "TOOL_APPROVAL_RESPONDED", 
      handler,
      { executionId }
    );
    
    if (timeoutMs > 0) {
      timeoutId = setTimeout(() => {
        unsubscribe(); // Manual cleanup on timeout
        reject(new Error(`User interaction timeout after ${timeoutMs}ms`));
      }, timeoutMs);
    }
  });
}
```

#### 2.3 UserInteractionResourceAPI

**File**: `sdk/api/workflow/resources/user-interaction/user-interaction-resource-api.ts`

**Before**:
```typescript
onToolApprovalRequested(listener: (event: ToolApprovalRequestedEvent) => void): void {
  this.dependencies.getEventManager().on("TOOL_APPROVAL_REQUESTED", listener);
}
```

**After**:
```typescript
onToolApprovalRequested(
  executionId: string,
  listener: (event: ToolApprovalRequestedEvent) => void
): () => void {
  return this.dependencies.getEventManager().on(
    "TOOL_APPROVAL_REQUESTED", 
    listener,
    { executionId }
  );
}

offToolApprovalRequested(
  executionId: string,
  listener: (event: ToolApprovalRequestedEvent) => void
): boolean {
  return this.dependencies.getEventManager().off(
    "TOOL_APPROVAL_REQUESTED", 
    listener
  );
}
```

---

### Phase 3: Update Utility Functions (Week 3)

#### 3.1 Event Waiter Functions

**File**: `sdk/workflow/execution/utils/event/event-waiter.ts`

**Update all waitFor functions to require executionId**:

```typescript
export async function waitForWorkflowExecutionCompleted(
  eventManager: EventRegistry,
  executionId: string, // Now required
  timeout: number = 5000,
): Promise<void> {
  const actualTimeout = timeout === WAIT_FOREVER ? undefined : timeout;
  
  await eventManager.waitFor(
    "WORKFLOW_EXECUTION_COMPLETED",
    actualTimeout,
    (event) => event.executionId === executionId
  );
}

// Similar updates for:
// - waitForWorkflowExecutionPaused
// - waitForWorkflowExecutionCancelled
// - waitForWorkflowExecutionFailed
// - waitForWorkflowExecutionResumed
// - waitForAnyLifecycleEvent
// - waitForNodeCompleted
// - waitForNodeFailed
```

#### 3.2 Event Emitter Utilities

**File**: `sdk/core/utils/event/event-emitter.ts`

No changes needed - emit functions remain the same. Events always include `executionId` in their payload.

---

### Phase 4: Update Tests (Week 3-4)

#### 4.1 Update Unit Tests

All test files using `eventManager.on()` must be updated:

**Example**: `sdk/core/coordinators/__tests__/followup-question-coordinator.test.ts`

**Before**:
```typescript
eventManager.on('FOLLOWUP_QUESTION_RESPONSE', (event: any) => {
  receivedEvent = event;
});
```

**After**:
```typescript
const executionId = 'test-execution-123';
eventManager.on('FOLLOWUP_QUESTION_RESPONSE', (event: any) => {
  receivedEvent = event;
}, { executionId });
```

#### 4.2 Add Validation Tests

Add tests to verify that missing `executionId` throws errors:

```typescript
it('should throw error when executionId is missing', () => {
  expect(() => {
    eventManager.on('NODE_COMPLETED', listener);
  }).toThrow(RuntimeValidationError);
  
  expect(() => {
    eventManager.once('NODE_COMPLETED', listener);
  }).toThrow(RuntimeValidationError);
});
```

---

### Phase 5: Documentation Updates (Week 4)

#### 5.1 Update Event System Guide

**File**: `sdk/docs/event-system-guide.md`

**Remove sections about global listeners**. Update all examples:

```markdown
## Quick Start

### Execution-scoped Listener (Required)

```typescript
const sdk = createSDK(options);
await sdk.waitForReady();

// Start workflow execution
const executionId = await sdk.workflows.execute(workflowId, input);

// Listen to events for this specific execution
sdk.events.on('NODE_COMPLETED', (event) => {
  console.log('Node completed:', event.nodeId);
}, { 
  executionId, // Required
  priority: 10 // Optional
});

// Listeners are automatically cleaned up when execution ends
```

### One-time Listener

```typescript
sdk.events.once('WORKFLOW_EXECUTION_COMPLETED', (event) => {
  console.log('Workflow completed');
}, { executionId }); // Required
```

### Waiting for Events

```typescript
// Wait for specific event
const event = await sdk.events.waitFor(
  'NODE_COMPLETED',
  5000, // timeout
  { executionId } // Required
);
```
```

#### 5.2 Update API Reference

Remove all references to global listeners from API documentation.

#### 5.3 Migration Guide

Create migration guide for existing code (even though we're skipping backward compatibility, users need guidance):

**File**: `sdk/docs/migration/event-system-v2.md`

```markdown
# Event System Migration Guide

## Breaking Changes

Global listeners have been removed. All event listeners must now specify an `executionId`.

## Migration Steps

### Step 1: Update Event Registration

**Before**:
```typescript
sdk.events.on('NODE_COMPLETED', handler);
```

**After**:
```typescript
const executionId = await sdk.workflows.execute(workflowId, input);
sdk.events.on('NODE_COMPLETED', handler, { executionId });
```

### Step 2: Update Coordinators

If you have custom coordinators using global listeners:

**Before**:
```typescript
class MyCoordinator {
  initialize() {
    this.eventManager.on('MY_EVENT', this.handler);
  }
}
```

**After**:
```typescript
class MyCoordinator {
  async handleEvent(executionId: string, data: any) {
    return new Promise((resolve) => {
      const unsubscribe = this.eventManager.on(
        'MY_EVENT_RESPONDED',
        (event) => {
          if (event.executionId === executionId) {
            unsubscribe();
            resolve(event.data);
          }
        },
        { executionId }
      );
      
      // Emit request
      this.emitMyEvent(executionId, data);
    });
  }
}
```

### Step 3: Replace Global Monitoring

For cross-execution monitoring, use structured logging instead:

**Before**:
```typescript
sdk.events.on('NODE_COMPLETED', (event) => {
  metricsCollector.record('node_completed', { nodeId: event.nodeId });
});
```

**After**:
```typescript
// In node execution coordinator
logger.info('Node completed', {
  nodeId: node.id,
  executionId: execution.id,
  workflowId: execution.workflowId,
  duration: executionTime,
});

// Use log aggregation tools (e.g., ELK, Datadog) for metrics
```
```

---

## Testing Strategy

### Unit Tests

1. **Validation Tests**: Verify `executionId` is required
2. **Cleanup Tests**: Verify listeners are cleaned up when execution ends
3. **Isolation Tests**: Verify listeners only receive events for their execution

### Integration Tests

1. **Multi-execution Test**: Run multiple executions simultaneously, verify no cross-talk
2. **Memory Leak Test**: Run 100+ executions, verify no listener accumulation
3. **Timeout Test**: Verify timeout cleanup works correctly

### Performance Tests

1. **Listener Registration**: Measure overhead of execution-scoped vs global
2. **Event Emission**: Verify no performance degradation
3. **Cleanup Performance**: Measure cleanup time for executions with many listeners

---

## Rollout Plan

### Week 1: Core Implementation
- [ ] Modify EventRegistry to require executionId
- [ ] Update Subscription classes
- [ ] Add validation and error messages

### Week 2: Internal Refactoring
- [ ] Refactor FollowupQuestionCoordinator
- [ ] Refactor LLMExecutionCoordinator
- [ ] Refactor UserInteractionResourceAPI
- [ ] Update all internal event usage

### Week 3: Utilities and Tests
- [ ] Update event waiter functions
- [ ] Update all unit tests
- [ ] Add new validation tests
- [ ] Fix any failing tests

### Week 4: Documentation and Review
- [ ] Update all documentation
- [ ] Create migration guide
- [ ] Code review
- [ ] Performance testing
- [ ] Final QA

---

## Risk Mitigation

### Risk 1: Breaking Existing Integrations

**Mitigation**:
- Clear error messages guide users to add executionId
- Comprehensive migration guide
- Example code for common patterns

### Risk 2: Performance Impact

**Mitigation**:
- Execution-scoped listeners use same underlying mechanism
- No additional overhead expected
- Performance tests will validate

### Risk 3: Complexity Increase

**Mitigation**:
- Simplified mental model (always execution-scoped)
- Automatic cleanup reduces manual management
- Better debugging with clear execution boundaries

---

## Success Metrics

1. **Zero Global Listeners**: Codebase contains no global listener registrations
2. **Memory Stability**: Long-running tests show no listener accumulation
3. **Test Coverage**: 100% of event-related tests pass
4. **Documentation Complete**: All APIs documented with executionId requirement
5. **Performance Parity**: No measurable performance degradation

---

## Appendix

### A. Files to Modify

**Core**:
- `sdk/core/registry/event-registry.ts`
- `sdk/core/coordinators/followup-question-coordinator.ts`
- `sdk/core/coordinators/tool-approval-coordinator.ts`

**API Layer**:
- `sdk/api/shared/types/subscription.ts`
- `sdk/api/workflow/operations/events/on-event-subscription.ts`
- `sdk/api/workflow/operations/events/once-event-subscription.ts`
- `sdk/api/workflow/resources/user-interaction/user-interaction-resource-api.ts`

**Utilities**:
- `sdk/workflow/execution/utils/event/event-waiter.ts`
- `sdk/core/utils/event/event-emitter.ts` (no changes, just review)

**Tests**:
- All files in `sdk/**/__tests__/*.test.ts` using eventManager.on()
- All files in `sdk/**/__tests__/*.int.test.ts` using events

**Documentation**:
- `sdk/docs/event-system-guide.md`
- `sdk/docs/execution-scoped-listeners-design.md`
- `sdk/docs/migration/event-system-v2.md` (new file)

### B. Related Issues

- Memory leak from unclosed global listeners
- Cross-execution event interference
- Difficulty debugging execution-specific issues
- Inconsistent listener lifecycle management

### C. References

- [Event Registry Implementation](file://d:\项目\agent\wf-agent\sdk\core\registry\event-registry.ts)
- [Subscription Pattern](file://d:\项目\agent\wf-agent\sdk\api\shared\types\subscription.ts)
- [Current Event System Guide](file://d:\项目\agent\wf-agent\sdk\docs\event-system-guide.md)
