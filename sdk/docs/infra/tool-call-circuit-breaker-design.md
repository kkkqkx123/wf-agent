# Tool Call Circuit Breaker Design

## 1. Overview

This document describes the design for implementing a circuit breaker mechanism that detects when an execution instance (Agent Loop or Workflow) consecutively calls the same tool and fails 3 times, then triggers circuit breaking to prevent further failures.

### 1.1 Problem Statement

Currently, when an Agent Loop or Workflow execution encounters repeated tool call failures:

- The system continues to retry the same failing tool
- No mechanism exists to detect consecutive failures of the same tool
- Resources are wasted on futile retry attempts
- Users receive no feedback about persistent tool issues

### 1.2 Solution Goals

1. **Detection**: Track consecutive failures of the same tool within an execution instance
2. **Threshold**: Trigger circuit breaker after 3 consecutive failures of the same tool
3. **Isolation**: Circuit breaker state is per-execution-instance (not global)
4. **Recovery**: Provide mechanisms to reset or bypass the circuit breaker
5. **Observability**: Emit events and logs for monitoring and debugging

## 2. Architecture

### 2.1 Integration Points

The circuit breaker integrates at three levels:

```
┌─────────────────────────────────────────┐
│   Execution Instance Level              │
│   - AgentLoopEntity                     │
│   - WorkflowExecutionEntity             │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│   State Manager                         │
│   - AgentLoopState                      │
│   - WorkflowExecutionState              │
│   - CircuitBreakerState (NEW)           │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│   Tool Execution Layer                  │
│   - ToolCallExecutor                    │
│   - CircuitBreakerManager (NEW)         │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│   Event System                          │
│   - CircuitBreakerTriggeredEvent        │
│   - CircuitBreakerResetEvent            │
└─────────────────────────────────────────┘
```

### 2.2 Key Components

#### 2.2.1 CircuitBreakerState

Tracks the circuit breaker state for each execution instance.

**Location**: `packages/types/src/agent-execution/types.ts` and `packages/types/src/workflow-execution/types.ts`

```typescript
/**
 * Circuit Breaker Status
 */
export enum CircuitBreakerStatus {
  /** Normal operation, no restrictions */
  CLOSED = "CLOSED",
  /** Circuit is open, tool calls are blocked */
  OPEN = "OPEN",
  /** Half-open, testing if tool recovers */
  HALF_OPEN = "HALF_OPEN",
}

/**
 * Circuit Breaker Record for a specific tool
 */
export interface CircuitBreakerRecord {
  /** Tool name */
  toolName: string;
  /** Current status */
  status: CircuitBreakerStatus;
  /** Consecutive failure count */
  consecutiveFailures: number;
  /** Last failure timestamp */
  lastFailureTime?: number;
  /** Circuit opened timestamp */
  openedAt?: number;
  /** Reason for opening */
  reason?: string;
}

/**
 * Circuit Breaker State
 * Tracks all circuit breakers within an execution instance
 */
export interface CircuitBreakerState {
  /** Map of tool name to circuit breaker record */
  records: Map<string, CircuitBreakerRecord>;
  /** Maximum consecutive failures before opening circuit */
  maxConsecutiveFailures: number;
  /** Cooldown period in milliseconds before attempting recovery */
  cooldownPeriodMs: number;
}
```

#### 2.2.2 CircuitBreakerManager

Manages circuit breaker logic during tool execution.

**Location**: `sdk/core/executors/circuit-breaker-manager.ts`

**Responsibilities**:

- Check if a tool call should be allowed
- Record tool call success/failure
- Update circuit breaker state
- Determine when to transition between states
- Emit circuit breaker events

```typescript
export class CircuitBreakerManager {
  private state: CircuitBreakerState;
  private eventManager?: EventRegistry;
  private executionId: string;
  private executionType: "AGENT_LOOP" | "WORKFLOW";

  constructor(
    executionId: string,
    executionType: "AGENT_LOOP" | "WORKFLOW",
    config?: Partial<CircuitBreakerState>,
    eventManager?: EventRegistry,
  );

  /**
   * Check if a tool call is allowed
   * Returns false if circuit is open for this tool
   */
  canExecuteTool(toolName: string): boolean;

  /**
   * Record a successful tool execution
   * Resets consecutive failure count and may close circuit
   */
  recordSuccess(toolName: string): void;

  /**
   * Record a failed tool execution
   * Increments consecutive failure count and may open circuit
   */
  recordFailure(toolName: string, error: Error): void;

  /**
   * Get current circuit breaker status for a tool
   */
  getStatus(toolName: string): CircuitBreakerStatus;

  /**
   * Manually reset circuit breaker for a tool
   */
  reset(toolName: string): void;

  /**
   * Reset all circuit breakers
   */
  resetAll(): void;

  /**
   * Get all circuit breaker records
   */
  getAllRecords(): CircuitBreakerRecord[];
}
```

#### 2.2.3 State Transitions

```
CLOSED ──[failure]──> CLOSED (consecutiveFailures < 3)
CLOSED ──[failure]──> OPEN (consecutiveFailures >= 3)
OPEN ──[cooldown expired]──> HALF_OPEN
HALF_OPEN ──[success]──> CLOSED
HALF_OPEN ──[failure]──> OPEN
```

## 3. Event System Extension

### 3.1 New Event Types

**Location**: `packages/types/src/events/tool-events.ts`

```typescript
/**
 * Circuit Breaker Triggered Event
 * Emitted when a circuit breaker opens due to consecutive failures
 */
export interface CircuitBreakerTriggeredEvent extends BaseEvent {
  type: "TOOL_CIRCUIT_BREAKER_TRIGGERED";
  /** Execution ID (Agent Loop or Workflow) */
  executionId: ID;
  /** Execution type */
  executionType: "AGENT_LOOP" | "WORKFLOW";
  /** Tool name */
  toolName: string;
  /** Consecutive failure count */
  consecutiveFailures: number;
  /** Last error message */
  lastError: string;
  /** Timestamp when circuit opened */
  openedAt: number;
  /** Cooldown period in milliseconds */
  cooldownPeriodMs: number;
  /** Node ID (if applicable) */
  nodeId?: ID;
  /** Agent Loop ID (if applicable) */
  agentLoopId?: ID;
}

/**
 * Circuit Breaker Reset Event
 * Emitted when a circuit breaker is reset (manually or after recovery)
 */
export interface CircuitBreakerResetEvent extends BaseEvent {
  type: "TOOL_CIRCUIT_BREAKER_RESET";
  /** Execution ID (Agent Loop or Workflow) */
  executionId: ID;
  /** Execution type */
  executionType: "AGENT_LOOP" | "WORKFLOW";
  /** Tool name */
  toolName: string;
  /** Reason for reset */
  reason: "manual" | "recovery" | "execution_end";
  /** Timestamp */
  resetAt: number;
  /** Node ID (if applicable) */
  nodeId?: ID;
  /** Agent Loop ID (if applicable) */
  agentLoopId?: ID;
}

/**
 * Circuit Breaker Half-Open Event
 * Emitted when circuit transitions to half-open state for testing
 */
export interface CircuitBreakerHalfOpenEvent extends BaseEvent {
  type: "TOOL_CIRCUIT_BREAKER_HALF_OPEN";
  /** Execution ID (Agent Loop or Workflow) */
  executionId: ID;
  /** Execution type */
  executionType: "AGENT_LOOP" | "WORKFLOW";
  /** Tool name */
  toolName: string;
  /** Timestamp */
  timestamp: number;
  /** Node ID (if applicable) */
  nodeId?: ID;
  /** Agent Loop ID (if applicable) */
  agentLoopId?: ID;
}
```

### 3.2 Event Builder Functions

**Location**: `sdk/core/utils/event/builders/tool-events.ts`

```typescript
/**
 * Build TOOL_CIRCUIT_BREAKER_TRIGGERED event
 */
export const buildCircuitBreakerTriggeredEvent = createBuilder<CircuitBreakerTriggeredEvent>(
  "TOOL_CIRCUIT_BREAKER_TRIGGERED",
);

/**
 * Build TOOL_CIRCUIT_BREAKER_RESET event
 */
export const buildCircuitBreakerResetEvent = createBuilder<CircuitBreakerResetEvent>(
  "TOOL_CIRCUIT_BREAKER_RESET",
);

/**
 * Build TOOL_CIRCUIT_BREAKER_HALF_OPEN event
 */
export const buildCircuitBreakerHalfOpenEvent = createBuilder<CircuitBreakerHalfOpenEvent>(
  "TOOL_CIRCUIT_BREAKER_HALF_OPEN",
);
```

### 3.3 Type Guards

**Location**: `packages/types/src/events/type-guards.ts`

```typescript
/**
 * Type guard for circuit breaker events
 */
export function isCircuitBreakerEvent(
  event: Event,
): event is CircuitBreakerTriggeredEvent | CircuitBreakerResetEvent | CircuitBreakerHalfOpenEvent {
  return (
    event.type === "TOOL_CIRCUIT_BREAKER_TRIGGERED" ||
    event.type === "TOOL_CIRCUIT_BREAKER_RESET" ||
    event.type === "TOOL_CIRCUIT_BREAKER_HALF_OPEN"
  );
}
```

## 4. Integration with Agent Loop

### 4.1 AgentLoopState Extension

**Location**: `sdk/agent/state-managers/agent-loop-state.ts`

Add circuit breaker state tracking:

```typescript
export class AgentLoopState implements StateManager<AgentLoopStateSnapshot> {
  // ... existing fields ...

  /** Circuit breaker state */
  private _circuitBreakerState: CircuitBreakerState;

  /** Circuit breaker manager */
  private _circuitBreakerManager?: CircuitBreakerManager;

  get circuitBreakerState(): CircuitBreakerState {
    return this._circuitBreakerState;
  }

  getCircuitBreakerManager(): CircuitBreakerManager | undefined {
    return this._circuitBreakerManager;
  }

  initializeCircuitBreakerManager(
    agentLoopId: string,
    eventManager?: EventRegistry,
    config?: Partial<CircuitBreakerState>,
  ): void {
    this._circuitBreakerManager = new CircuitBreakerManager(
      agentLoopId,
      "AGENT_LOOP",
      config,
      eventManager,
    );
  }
}
```

### 4.2 AgentLoopEntity Integration

**Location**: `sdk/agent/entities/agent-loop-entity.ts`

Initialize circuit breaker manager when creating AgentLoopEntity:

```typescript
constructor(config: AgentLoopRuntimeConfig, state: AgentLoopState) {
  // ... existing initialization ...

  // Initialize circuit breaker manager
  state.initializeCircuitBreakerManager(
    this.id,
    config.eventManager,
    config.circuitBreakerConfig
  );
}
```

### 4.3 Tool Execution Hook

**Location**: `sdk/core/executors/tool-call-executor.ts`

Modify `executeSingleToolCall` to check circuit breaker before execution:

```typescript
private async executeSingleToolCall(
  toolCall: { id: string; name: string; arguments: string },
  conversationState: ConversationSession,
  executionId: string | undefined,
  nodeId: string | undefined,
  batchId: string,
  taskInfo: ToolCallTaskInfo,
  options?: { abortSignal?: AbortSignal },
): Promise<ToolExecutionResult> {
  // Check circuit breaker before execution
  const circuitBreakerManager = this.getCircuitBreakerManager(executionId);
  if (circuitBreakerManager && !circuitBreakerManager.canExecuteTool(toolCall.name)) {
    const errorMessage = `Tool '${toolCall.name}' is currently blocked by circuit breaker due to consecutive failures`;

    // Emit circuit breaker triggered event if not already emitted
    // Return failure result immediately without executing tool
    return {
      toolCallId: toolCall.id,
      toolId: toolCall.name,
      success: false,
      error: errorMessage,
      executionTime: 0,
      retryCount: 0,
    };
  }

  try {
    // ... existing tool execution logic ...

    const result = await this.toolService.execute(...);

    // Record success
    if (circuitBreakerManager) {
      circuitBreakerManager.recordSuccess(toolCall.name);
    }

    return result;
  } catch (error) {
    // Record failure
    if (circuitBreakerManager) {
      circuitBreakerManager.recordFailure(toolCall.name, error as Error);
    }

    throw error;
  }
}
```

## 5. Integration with Workflow Execution

### 5.1 WorkflowExecutionState Extension

**Location**: `sdk/workflow/state-managers/workflow-execution-state.ts`

Similar to AgentLoopState, add circuit breaker support:

```typescript
export class WorkflowExecutionState implements StateManager<WorkflowExecutionStateSnapshot> {
  // ... existing fields ...

  /** Circuit breaker state */
  private _circuitBreakerState: CircuitBreakerState;

  /** Circuit breaker manager */
  private _circuitBreakerManager?: CircuitBreakerManager;

  get circuitBreakerState(): CircuitBreakerState {
    return this._circuitBreakerState;
  }

  getCircuitBreakerManager(): CircuitBreakerManager | undefined {
    return this._circuitBreakerManager;
  }

  initializeCircuitBreakerManager(
    workflowExecutionId: string,
    eventManager?: EventRegistry,
    config?: Partial<CircuitBreakerState>,
  ): void {
    this._circuitBreakerManager = new CircuitBreakerManager(
      workflowExecutionId,
      "WORKFLOW",
      config,
      eventManager,
    );
  }
}
```

### 5.2 Workflow Node Handler Integration

For nodes that execute tools (e.g., LLM nodes with tool calls), integrate circuit breaker checks similar to Agent Loop integration.

## 6. Configuration

### 6.1 Default Configuration

```typescript
const DEFAULT_CIRCUIT_BREAKER_CONFIG: CircuitBreakerState = {
  records: new Map(),
  maxConsecutiveFailures: 3,
  cooldownPeriodMs: 60000, // 1 minute
};
```

### 6.2 Runtime Configuration

Configuration can be provided via:

1. **Agent Loop Config**: `AgentLoopRuntimeConfig.circuitBreakerConfig`
2. **Workflow Config**: Workflow metadata or node-level configuration
3. **Global Config**: Via DI container or global settings

Example:

```toml
# In agent profile or workflow config
[circuit_breaker]
max_consecutive_failures = 3
cooldown_period_ms = 60000
enabled = true
```

## 7. Serialization and Checkpoint Support

### 7.1 AgentLoopStateSnapshot Extension

**Location**: `packages/types/src/checkpoint/agent/snapshot.ts`

```typescript
export interface AgentLoopStateSnapshot {
  // ... existing fields ...

  /** Circuit breaker state (serializable) */
  circuitBreakerState?: {
    records: Array<{
      toolName: string;
      status: string;
      consecutiveFailures: number;
      lastFailureTime?: number;
      openedAt?: number;
      reason?: string;
    }>;
    maxConsecutiveFailures: number;
    cooldownPeriodMs: number;
  };
}
```

### 7.2 WorkflowExecutionStateSnapshot Extension

Similar extension for workflow execution snapshots.

### 7.3 Serialization Logic

In `AgentLoopState.createSnapshot()`:

```typescript
createSnapshot(): AgentLoopStateSnapshot {
  return {
    // ... existing snapshot data ...

    circuitBreakerState: this._circuitBreakerState ? {
      records: Array.from(this._circuitBreakerState.records.entries()).map(([toolName, record]) => ({
        toolName,
        status: record.status,
        consecutiveFailures: record.consecutiveFailures,
        lastFailureTime: record.lastFailureTime,
        openedAt: record.openedAt,
        reason: record.reason,
      })),
      maxConsecutiveFailures: this._circuitBreakerState.maxConsecutiveFailures,
      cooldownPeriodMs: this._circuitBreakerState.cooldownPeriodMs,
    } : undefined,
  };
}
```

In `AgentLoopState.restoreFromSnapshot()`:

```typescript
restoreFromSnapshot(snapshot: AgentLoopStateSnapshot): void {
  // ... existing restore logic ...

  if (snapshot.circuitBreakerState) {
    this._circuitBreakerState = {
      records: new Map(
        snapshot.circuitBreakerState.records.map(record => [
          record.toolName,
          {
            toolName: record.toolName,
            status: record.status as CircuitBreakerStatus,
            consecutiveFailures: record.consecutiveFailures,
            lastFailureTime: record.lastFailureTime,
            openedAt: record.openedAt,
            reason: record.reason,
          }
        ])
      ),
      maxConsecutiveFailures: snapshot.circuitBreakerState.maxConsecutiveFailures,
      cooldownPeriodMs: snapshot.circuitBreakerState.cooldownPeriodMs,
    };
  }
}
```

## 8. Recovery Mechanisms

### 8.1 Automatic Recovery (Half-Open State)

After cooldown period expires, circuit transitions to HALF_OPEN state and allows one test execution:

```typescript
private checkCooldown(record: CircuitBreakerRecord): void {
  if (record.status === CircuitBreakerStatus.OPEN && record.openedAt) {
    const elapsed = Date.now() - record.openedAt;
    if (elapsed >= this.state.cooldownPeriodMs) {
      record.status = CircuitBreakerStatus.HALF_OPEN;
      this.emitHalfOpenEvent(record.toolName);
    }
  }
}
```

### 8.2 Manual Reset API

Provide APIs to manually reset circuit breakers:

```typescript
// For Agent Loop
const entity = agentLoopCoordinator.get(agentLoopId);
entity.state.getCircuitBreakerManager()?.reset("problematic_tool");

// For Workflow
const entity = workflowExecutionCoordinator.get(workflowExecutionId);
entity.state.getCircuitBreakerManager()?.reset("problematic_tool");
```

### 8.3 Auto-Reset on Execution End

When an execution completes (successfully or with failure), all circuit breakers should be reset:

```typescript
// In AgentLoopState.complete() or fail()
this._circuitBreakerManager?.resetAll();

// In WorkflowExecutionState.complete() or fail()
this._circuitBreakerManager?.resetAll();
```

## 9. Error Handling and User Feedback

### 9.1 Error Messages

When a tool is blocked by circuit breaker, provide clear error messages:

```
Tool 'read_file' is currently unavailable due to 3 consecutive failures.
Circuit breaker will automatically reset after 60 seconds, or you can manually reset it.
Last error: File not found: /path/to/file.txt
```

### 9.2 Logging

Comprehensive logging at each state transition:

```typescript
logger.info("Circuit breaker opened", {
  executionId: this.executionId,
  executionType: this.executionType,
  toolName,
  consecutiveFailures: record.consecutiveFailures,
  lastError: error.message,
});

logger.info("Circuit breaker reset", {
  executionId: this.executionId,
  executionType: this.executionType,
  toolName,
  reason,
});
```

### 9.3 CLI/UI Integration

Expose circuit breaker status via CLI commands:

```bash
# Check circuit breaker status for an agent loop
wf-agent agent status --id <agent-loop-id> --show-circuit-breakers

# Manually reset a circuit breaker
wf-agent agent reset-circuit-breaker --id <agent-loop-id> --tool <tool-name>
```

## 10. Testing Strategy

### 10.1 Unit Tests

Test `CircuitBreakerManager` in isolation:

```typescript
describe("CircuitBreakerManager", () => {
  it("should allow tool execution when circuit is closed", () => {
    const manager = new CircuitBreakerManager("test-id", "AGENT_LOOP");
    expect(manager.canExecuteTool("test_tool")).toBe(true);
  });

  it("should open circuit after 3 consecutive failures", () => {
    const manager = new CircuitBreakerManager("test-id", "AGENT_LOOP");
    const error = new Error("Test error");

    manager.recordFailure("test_tool", error);
    manager.recordFailure("test_tool", error);
    manager.recordFailure("test_tool", error);

    expect(manager.getStatus("test_tool")).toBe(CircuitBreakerStatus.OPEN);
    expect(manager.canExecuteTool("test_tool")).toBe(false);
  });

  it("should reset consecutive failures on success", () => {
    const manager = new CircuitBreakerManager("test-id", "AGENT_LOOP");
    const error = new Error("Test error");

    manager.recordFailure("test_tool", error);
    manager.recordFailure("test_tool", error);
    manager.recordSuccess("test_tool");
    manager.recordFailure("test_tool", error);

    expect(manager.getStatus("test_tool")).toBe(CircuitBreakerStatus.CLOSED);
  });

  it("should transition to half-open after cooldown", async () => {
    const manager = new CircuitBreakerManager("test-id", "AGENT_LOOP", {
      cooldownPeriodMs: 100,
    });
    const error = new Error("Test error");

    manager.recordFailure("test_tool", error);
    manager.recordFailure("test_tool", error);
    manager.recordFailure("test_tool", error);

    expect(manager.getStatus("test_tool")).toBe(CircuitBreakerStatus.OPEN);

    await sleep(150);
    manager.canExecuteTool("test_tool"); // Triggers cooldown check

    expect(manager.getStatus("test_tool")).toBe(CircuitBreakerStatus.HALF_OPEN);
  });
});
```

### 10.2 Integration Tests

Test integration with Agent Loop and Workflow execution:

```typescript
describe("Circuit Breaker Integration", () => {
  it("should block tool execution in Agent Loop after consecutive failures", async () => {
    // Create agent loop with mock tool that always fails
    // Execute 3 iterations
    // Verify 4th attempt is blocked by circuit breaker
  });

  it("should emit circuit breaker events", async () => {
    // Subscribe to events
    // Trigger circuit breaker
    // Verify TOOL_CIRCUIT_BREAKER_TRIGGERED event was emitted
  });

  it("should persist circuit breaker state in checkpoint", async () => {
    // Create agent loop
    // Trigger circuit breaker
    // Create checkpoint
    // Restore from checkpoint
    // Verify circuit breaker state is restored
  });
});
```

## 11. Implementation Roadmap

### Phase 1: Core Infrastructure (Week 1)

- [ ] Define types in `packages/types/src/agent-execution/types.ts`
- [ ] Implement `CircuitBreakerManager` class
- [ ] Add event type definitions
- [ ] Implement event builders

### Phase 2: Agent Loop Integration (Week 2)

- [ ] Extend `AgentLoopState` with circuit breaker support
- [ ] Integrate with `ToolCallExecutor`
- [ ] Add serialization support
- [ ] Write unit tests

### Phase 3: Workflow Integration (Week 3)

- [ ] Extend `WorkflowExecutionState` with circuit breaker support
- [ ] Integrate with workflow node handlers
- [ ] Add serialization support
- [ ] Write integration tests

### Phase 4: CLI/UI and Documentation (Week 4)

- [ ] Add CLI commands for circuit breaker management
- [ ] Update API documentation
- [ ] Add user-facing error messages
- [ ] Write end-to-end tests

## 12. Considerations and Trade-offs

### 12.1 Per-Instance vs Global Circuit Breaker

**Decision**: Per-execution-instance

**Rationale**:

- Different executions may have different contexts and failure reasons
- A tool failing in one execution doesn't mean it's broken globally
- Provides better isolation and flexibility

### 12.2 Threshold Configuration

**Decision**: Configurable, default to 3

**Rationale**:

- 3 failures is a reasonable balance between early detection and avoiding false positives
- Allows customization for different use cases
- Can be adjusted based on tool criticality

### 12.3 Cooldown Period

**Decision**: Configurable, default to 60 seconds

**Rationale**:

- Gives time for transient issues to resolve
- Not too long to cause unnecessary delays
- Can be tuned based on typical failure recovery time

### 12.4 Half-Open State Complexity

**Decision**: Include half-open state

**Rationale**:

- Allows automatic recovery without manual intervention
- Tests if the issue is resolved before fully reopening
- Standard circuit breaker pattern

## 13. Future Enhancements

1. **Adaptive Thresholds**: Adjust threshold based on historical success rates
2. **Tool Health Metrics**: Track overall tool health across all executions
3. **Fallback Tools**: Automatically switch to alternative tools when circuit is open
4. **Predictive Circuit Breaking**: Use ML to predict failures before they occur
5. **Dashboard Visualization**: Real-time monitoring of circuit breaker states
6. **Alerting Integration**: Send alerts when circuit breakers trigger frequently

## 14. References

- Martin Fowler's Circuit Breaker Pattern: https://martinfowler.com/bliki/CircuitBreaker.html
- Resilience4j Circuit Breaker: https://resilience4j.readme.io/docs/circuitbreaker
- Netflix Hystrix: https://github.com/Netflix/Hystrix/wiki
