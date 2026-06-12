# Tool Failure Protection Design

## 1. Overview

This document describes a lightweight mechanism to protect execution instances from repeatedly calling failing tools. When the same tool fails consecutively within an execution instance, subsequent calls are temporarily blocked to prevent resource waste.

### Core Problem

When an Agent Loop or Workflow encounters repeated tool failures:

- The system continues retrying the same failing tool in each iteration
- No detection exists for consecutive failures of the same tool
- Resources are wasted on futile attempts
- Users receive no feedback about persistent issues

### Solution Approach

Track tool failure counts per execution instance and temporarily block tools after reaching a threshold. After a cooldown period, the tool becomes available again.

### Key Design Principle

**Per-Instance Isolation**: Each execution instance (Agent Loop or Workflow) maintains independent failure tracking. Parent workflows and triggered sub-workflows have completely separate protection states.

## 2. Architecture Decision: Separate State Module vs. Integrated State

### Analysis

The critical question is whether tool failure protection should be:

1. **Integrated** into existing state managers (`AgentLoopState`, `WorkflowExecutionState`)
2. **Separate** as an independent state module held by execution entities

### Current Architecture Patterns

Examining the existing codebase reveals two patterns:

**Pattern A: Integrated State (AgentLoopState)**

- `AgentLoopState` manages all execution state in one class
- Includes: status, iteration count, history, streaming state, pending tool calls
- All state is serialized together in checkpoints
- Simple but can become monolithic

**Pattern B: Composed State Managers (Workflow)**

- `WorkflowExecutionEntity` holds multiple independent state managers:
  - `WorkflowExecutionState`: Execution status and control flags
  - `VariableState`: Variable management (separate module)
  - `MessageHistory`: Message tracking (separate module)
  - `ExecutionState`: Subgraph stack management (separate module)
- Each manager has clear responsibility boundaries
- More modular and maintainable

### Decision: Separate State Module

**Rationale:**

1. **Responsibility Separation**: Tool failure protection is conceptually distinct from execution status tracking. Mixing them violates single responsibility principle.

2. **Modularity**: Following the Workflow pattern of composed state managers makes the architecture more consistent and easier to maintain.

3. **Independent Lifecycle**: Failure tracking may need different configuration, reset behavior, or serialization rules compared to core execution state.

4. **Reusability**: A separate module can be reused across Agent Loop and Workflow without duplicating logic in both state classes.

5. **Testing**: Isolated module is easier to test independently.

6. **Future Extensibility**: If we later add adaptive thresholds, per-tool configurations, or health metrics, having a dedicated module avoids bloating the main state classes.

### Implementation Strategy

Create `ToolFailureProtectionState` as an independent state manager that:

- Implements the `StateManager` interface
- Is held by execution entities (`AgentLoopEntity`, `WorkflowExecutionEntity`)
- Can be composed with other state managers
- Serializes independently in checkpoints

## 3. Component Design

### 3.1 ToolFailureProtectionState

A standalone state manager responsible for tracking tool failures and determining if a tool should be blocked.

**Responsibilities:**

- Track consecutive failure counts per tool
- Determine if a tool can be executed based on failure history
- Record successful and failed executions
- Manage cooldown periods for blocked tools
- Provide serialization support for checkpoints

**Key Characteristics:**

- Independent lifecycle from execution state
- Configuration-driven behavior (thresholds, cooldown periods)
- Pure state management with no side effects
- Serializable for checkpoint support

### 3.2 Ownership Model

Each execution entity owns its own `ToolFailureProtectionState` instance:

**Agent Loop:**

```
AgentLoopEntity
  ├─ config: AgentLoopRuntimeConfig
  ├─ state: AgentLoopState
  ├─ conversationManager: ConversationSession
  └─ toolFailureProtection: ToolFailureProtectionState  ← New
```

**Workflow:**

```
WorkflowExecutionEntity
  ├─ workflowExecution: WorkflowExecution (data object)
  ├─ state: WorkflowExecutionState
  ├─ messageHistoryManager: MessageHistory
  ├─ variableStateManager: VariableState
  ├─ executionState: ExecutionState
  └─ toolFailureProtection: ToolFailureProtectionState  ← New
```

### 3.3 Hierarchy Isolation Guarantee

**Critical Requirement:** Parent and child workflows must have completely independent failure tracking.

**How It Works:**

When a parent workflow triggers a sub-workflow:

1. A new `WorkflowExecutionEntity` is created with a unique ID
2. This new entity gets a fresh `ToolFailureProtectionState` instance
3. The fresh state starts with empty failure counts
4. Parent's failure tracking has zero impact on child's ability to use tools

**Example Scenario:**

Parent workflow calls `read_file` three times, all fail → tool blocked in parent.

Parent then triggers sub-workflow.

Sub-workflow has its own `ToolFailureProtectionState` with zero failures → `read_file` is fully available in sub-workflow.

This isolation is automatic because each execution entity creates its own state instance during construction.

### 3.4 Integration with ToolCallExecutor

`ToolCallExecutor` receives an optional state manager parameter when executing tools:

**Flow:**

1. Before executing a tool, check with the state manager if allowed
2. If blocked, return immediately with appropriate error message and emit event
3. If allowed, proceed with execution
4. After execution, record success or failure with the state manager
5. State manager updates internal tracking accordingly

**Interface Contract:**

The executor expects a simple interface with three methods:

- Check if tool can execute
- Record successful execution
- Record failed execution

This keeps the executor decoupled from the specific implementation.

## 4. Behavior Specification

### 4.1 Failure Tracking Logic

**Initial State:**

- All tools start with zero failure count
- All tools are executable

**On Tool Failure:**

- Increment failure count for that specific tool
- Update last failure timestamp
- Store error message for debugging

**On Tool Success:**

- Reset failure count to zero for that tool
- Clear any blocking state

**Blocking Decision:**

- If failure count reaches threshold (default: 3), block the tool
- Calculate remaining cooldown time
- Block persists until cooldown expires

**Recovery:**

- After cooldown period (default: 60 seconds), automatically unblock
- Reset failure count on next successful execution
- Manual reset also supported via API

### 4.2 Configuration Parameters

Three configurable parameters control behavior:

**Max Consecutive Failures:**

- Default: 3
- Determines how many failures trigger blocking
- Can be adjusted per execution instance

**Cooldown Period:**

- Default: 60 seconds (60000 milliseconds)
- Duration a tool remains blocked after reaching threshold
- Allows transient issues to resolve

**Enabled Flag:**

- Default: true
- Can disable protection entirely if needed
- Useful for testing or special scenarios

### 4.3 Edge Cases

**Mixed Success/Failure Pattern:**
If a tool fails twice, succeeds once, then fails again, the count resets to one (not three). Only consecutive failures matter.

**Multiple Tools:**
Each tool tracks failures independently. One blocked tool does not affect others.

**Checkpoint Restoration:**
When restoring from checkpoint, failure counts are restored to their saved state. This means:

- If a tool was blocked before checkpoint, it remains blocked after restore
- Cooldown timers continue from where they left off (based on timestamps)
- This is correct behavior - we don't want to lose protection state

**Execution Completion:**
When an execution completes (success or failure), all failure counts are cleared. This prevents stale state from affecting future runs.

## 5. Event System Integration

### 5.1 Event Emission Points

Two key events are emitted:

**Tool Call Blocked:**

- Triggered when a tool is prevented from executing due to failure protection
- Includes: execution ID, tool name, failure count, last error, remaining cooldown
- Allows monitoring and user notification

**Tool Call Failed:**

- Existing event, enhanced with failure count information
- Helps correlate individual failures with blocking decisions

### 5.2 Event Consumption

Applications can listen for these events to:

- Display warnings to users
- Log metrics for analysis
- Trigger alternative actions (e.g., switch to backup tool)
- Send alerts for frequently blocked tools

## 6. Checkpoint Integration

### 6.1 Serialization Requirements

`ToolFailureProtectionState` must support snapshot creation and restoration:

**Snapshot Contents:**

- Failure count map (tool name → count, timestamp, last error)
- Configuration parameters
- Timestamps for cooldown calculation

**Serialization Format:**

- Convert Map to array of entries for JSON compatibility
- Include all necessary fields for accurate restoration
- Keep format stable for backward compatibility

### 6.2 Restoration Behavior

When restoring from checkpoint:

1. Reconstruct failure count map from snapshot
2. Restore configuration parameters
3. Calculate current blocking state based on timestamps
4. Continue protection as if execution never paused

This ensures protection state survives pause/resume cycles.

## 7. Testing Strategy

### 7.1 Unit Tests

Test `ToolFailureProtectionState` in isolation:

**Basic Functionality:**

- Initial state allows all tools
- Failure count increments correctly
- Success resets failure count
- Blocking occurs at threshold
- Recovery happens after cooldown

**Configuration:**

- Custom thresholds work correctly
- Cooldown periods are respected
- Enable/disable flag functions properly

**Edge Cases:**

- Mixed success/failure patterns
- Multiple tools tracked independently
- Timestamp-based cooldown calculations

### 7.2 Integration Tests

Test integration with execution entities:

**Agent Loop:**

- Create entity with protection state
- Execute iterations with failing tool
- Verify blocking occurs after threshold
- Confirm events are emitted

**Workflow:**

- Similar tests for workflow execution
- Verify LLM nodes respect blocking

**Hierarchy Isolation:**

- Create parent workflow with blocked tool
- Trigger sub-workflow
- Verify sub-workflow can use the tool freely
- Confirm states are completely independent

**Checkpoint:**

- Create execution with failure state
- Create checkpoint
- Restore from checkpoint
- Verify protection state is preserved
- Confirm blocking behavior continues correctly

### 7.3 End-to-End Tests

Full scenario tests:

**Agent Loop Scenario:**

1. Create agent loop with mock failing tool
2. Execute multiple iterations
3. Verify tool gets blocked
4. Wait for cooldown
5. Verify tool becomes available again

**Nested Workflow Scenario:**

1. Create parent workflow with failing tool
2. Block tool in parent
3. Trigger sub-workflow
4. Verify sub-workflow unaffected
5. Both continue execution appropriately

## 8. Implementation Phases

### Phase 1: Core Module (Week 1)

- Create `ToolFailureProtectionState` class
- Implement failure tracking logic
- Add configuration support
- Write unit tests
- Define snapshot interface

### Phase 2: Entity Integration (Week 1-2)

- Add state to `AgentLoopEntity`
- Add state to `WorkflowExecutionEntity`
- Update entity constructors
- Add cleanup logic
- Test basic integration

### Phase 3: Executor Integration (Week 2)

- Modify `ToolCallExecutor` to accept state manager
- Implement pre-execution checks
- Implement post-execution recording
- Add event emission
- Write integration tests

### Phase 4: Checkpoint Support (Week 2-3)

- Implement snapshot creation
- Implement snapshot restoration
- Test checkpoint round-trip
- Verify state preservation
- Test pause/resume scenarios

### Phase 5: Hierarchy Validation (Week 3)

- Test parent-child isolation
- Verify triggered sub-workflows
- Test nested scenarios
- Confirm no cross-contamination
- Write comprehensive isolation tests

### Phase 6: Polish and Documentation (Week 3-4)

- Add CLI commands for inspection/reset
- Update API documentation
- Add error messages
- Final integration testing
- Update architecture docs

## 9. Trade-offs and Considerations

### 9.1 Separate Module vs. Integrated State

**Separate Module Advantages:**

- Cleaner separation of concerns
- Follows existing Workflow pattern
- Easier to test and maintain
- More flexible for future changes
- Consistent with composed state manager approach

**Separate Module Disadvantages:**

- Slightly more files to manage
- Need to ensure proper initialization
- One additional dependency in entities

**Conclusion:** Benefits significantly outweigh costs. The modularity and consistency with existing patterns make this the better choice.

### 9.2 Per-Instance vs. Global Tracking

**Decision:** Strictly per-instance

**Rationale:**

- Different executions have different contexts
- A tool failing in one execution doesn't indicate global problems
- Critical for nested workflow isolation
- Simpler implementation (no synchronization needed)

### 9.3 Threshold Selection

**Default:** 3 consecutive failures

**Rationale:**

- Balances early detection vs. false positives
- Allows for transient network issues
- Not too aggressive to block prematurely
- Configurable for different needs

### 9.4 Cooldown Duration

**Default:** 60 seconds

**Rationale:**

- Gives time for most transient issues to resolve
- Not so long as to cause excessive delays
- Can be tuned based on typical failure patterns
- Reasonable default for most scenarios

## 10. Future Enhancements

Potential improvements after initial implementation:

1. **Adaptive Thresholds:** Adjust based on historical success rates
2. **Per-Tool Configuration:** Different thresholds for different tools
3. **Smart Cooldown:** Adaptive cooldown based on error types
4. **Fallback Tools:** Automatically switch to alternatives when blocked
5. **Health Metrics:** Track overall tool health across executions
6. **Dashboard:** Real-time visualization of failure states
7. **Alerting:** Notifications for frequently blocked tools
8. **Error Classification:** Distinguish between transient and permanent failures

## 11. Summary

This design provides a lightweight, modular solution for protecting execution instances from repeatedly calling failing tools. By implementing it as a separate state module following the existing composed state manager pattern, we achieve:

- Clean separation of concerns
- Automatic hierarchy isolation
- Easy testing and maintenance
- Consistent architecture
- Flexible configuration
- Checkpoint support

The per-instance isolation guarantee is particularly important for nested workflow scenarios, ensuring that parent and child workflows maintain independent protection states without any manual intervention.
