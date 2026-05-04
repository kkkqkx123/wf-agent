# Task 3.2: Mid-Node Resume Implementation Plan

## Executive Summary

This document provides a detailed implementation plan for enabling mid-node resume functionality in the workflow execution system. This advanced feature allows workflows to resume from the middle of long-running operations (LLM streaming, tool execution) rather than restarting entire nodes.

**Complexity:** High  
**Estimated Effort:** 6-8 hours  
**Priority:** 🟢 Low (Deferred - implement after core pause/resume is stable)

---

## Problem Statement

### Current Limitation

When a workflow is paused during a long-running operation:
1. **LLM Streaming:** If paused during LLM response streaming, the entire LLM call restarts on resume
2. **Tool Execution:** If paused during tool execution, the tool call restarts from beginning
3. **Wasted Resources:** Partial results are lost and recomputed
4. **Poor UX:** Users experience delays when resuming complex operations

### Example Scenarios

```typescript
// Scenario 1: LLM Streaming Interruption
// User pauses at 50% of LLM response
[Node: LLM] ----50% streamed---- [PAUSE]
On resume: Restarts entire LLM call ❌

// Scenario 2: Tool Execution Interruption  
// User pauses during file processing tool
[Node: Tool] --processing 100 files, 50 done-- [PAUSE]
On resume: Reprocesses all 100 files ❌

// Desired Behavior:
[Node: LLM] ----50% streamed---- [PAUSE]
On resume: Continue streaming from 50% ✅

[Node: Tool] --processing 100 files, 50 done-- [PAUSE]
On resume: Continue from file 51 ✅
```

---

## Architecture Analysis

### Current State Management

#### Agent Module (Reference Implementation)
The agent module already implements partial state tracking:

```typescript
// sdk/agent/entities/agent-loop-state.ts
class AgentLoopState {
  private _streamMessage: LLMMessage | null = null;  // Partial message
  private _pendingToolCalls: Set<string> = new Set(); // In-flight tools
  private _isStreaming: boolean = false;              // Streaming flag
  
  // Serialization support
  toSnapshot(): AgentLoopStateSnapshot {
    return {
      streamMessage: this._streamMessage,
      pendingToolCalls: Array.from(this._pendingToolCalls),
      isStreaming: this._isStreaming,
      // ... other fields
    };
  }
}
```

#### Workflow Module (Current Gap)
WorkflowExecutionState does NOT track operation-level state:

```typescript
// sdk/workflow/state-managers/workflow-execution-state.ts
class WorkflowExecutionState {
  // Only tracks high-level state
  private _status: WorkflowExecutionStatus;
  private _currentNodeId: string;
  // ❌ No tracking of:
  // - LLM streaming progress
  // - Tool execution progress
  // - Partial results
}
```

### Checkpoint System Capabilities

Current checkpoint stores:
```typescript
interface CheckpointMetadata {
  customFields?: Record<string, unknown>;
  // Can store arbitrary data!
}

// Checkpoint already supports storing operation state via customFields
// We just need to populate it with operation-level details
```

---

## Implementation Strategy

### Phase 1: Extend State Tracking (2-3 hours)

#### Step 1.1: Add Operation State to WorkflowExecutionState

**File:** `sdk/workflow/state-managers/workflow-execution-state.ts`

```typescript
/**
 * Operation-level execution state
 * Tracks progress within individual node operations
 */
export interface OperationState {
  /** Type of operation */
  type: "LLM_STREAMING" | "TOOL_EXECUTION" | "SCRIPT_EXECUTION";
  
  /** Operation ID (e.g., toolCallId, requestId) */
  operationId: string;
  
  /** Node ID where operation is running */
  nodeId: string;
  
  /** Start timestamp */
  startedAt: number;
  
  /** Progress information (operation-specific) */
  progress?: {
    /** For LLM: tokens generated so far */
    tokensGenerated?: number;
    /** For tools: items processed / total items */
    itemsProcessed?: number;
    totalItems?: number;
    /** Generic progress percentage (0-100) */
    percentage?: number;
  };
  
  /** Partial result accumulated so far */
  partialResult?: unknown;
  
  /** Operation-specific metadata */
  metadata?: Record<string, unknown>;
}

export class WorkflowExecutionState {
  // ... existing fields ...
  
  /** Current operation state (if any) */
  private _currentOperation: OperationState | null = null;
  
  /**
   * Get current operation state
   */
  getCurrentOperation(): OperationState | null {
    return this._currentOperation;
  }
  
  /**
   * Set current operation state
   */
  setCurrentOperation(operation: OperationState | null): void {
    this._currentOperation = operation;
  }
  
  /**
   * Update operation progress
   */
  updateOperationProgress(progress: Partial<OperationState["progress"]>): void {
    if (this._currentOperation) {
      this._currentOperation.progress = {
        ...this._currentOperation.progress,
        ...progress,
      };
    }
  }
  
  /**
   * Update partial result
   */
  updatePartialResult(result: unknown): void {
    if (this._currentOperation) {
      this._currentOperation.partialResult = result;
    }
  }
  
  /**
   * Clear operation state (when operation completes)
   */
  clearOperation(): void {
    this._currentOperation = null;
  }
  
  /**
   * Serialize operation state for checkpoint
   */
  getOperationStateSnapshot(): OperationState | null {
    return this._currentOperation ? { ...this._currentOperation } : null;
  }
  
  /**
   * Restore operation state from checkpoint
   */
  restoreOperationState(snapshot: OperationState | null): void {
    this._currentOperation = snapshot ? { ...snapshot } : null;
  }
}
```

#### Step 1.2: Update Checkpoint Creation to Include Operation State

**File:** `sdk/workflow/checkpoint/checkpoint-coordinator.ts`

```typescript
static async createCheckpoint(
  workflowExecutionId: string,
  dependencies: CheckpointDependencies,
  metadata?: CheckpointMetadata,
): Promise<string> {
  
  // Capture operation state
  const operationState = workflowExecutionEntity.state.getOperationStateSnapshot();
  
  // Build complete state snapshot
  const stateSnapshot: WorkflowExecutionStateSnapshot = {
    status: workflowExecutionEntity.getStatus(),
    currentNodeId: workflowExecutionEntity.getCurrentNodeId(),
    nodeResults: workflowExecutionEntity.getNodeResults(),
    variables: workflowExecutionEntity.getAllVariables(),
    messages: conversationManager?.getMessages() || [],
    subgraphStack: workflowExecutionEntity.getSubgraphStack(),
    forkJoinContext: workflowExecutionEntity.getForkJoinContext(),
    triggeredSubworkflowContext: workflowExecutionEntity.getTriggeredSubworkflowContext(),
    // NEW: Include operation state
    currentOperation: operationState,
  };
  
  // ... rest of checkpoint creation ...
}
```

#### Step 1.3: Update Checkpoint Restoration

**File:** `sdk/workflow/checkpoint/checkpoint-coordinator.ts`

```typescript
static async restoreFromCheckpoint(
  checkpointId: string,
  dependencies: CheckpointDependencies,
): Promise<{...}> {
  
  // Restore operation state
  if (workflowExecutionState.currentOperation) {
    workflowExecutionEntity.state.restoreOperationState(
      workflowExecutionState.currentOperation
    );
    
    logger.info("Restored operation state from checkpoint", {
      executionId: checkpoint.executionId,
      operationType: workflowExecutionState.currentOperation.type,
      operationId: workflowExecutionState.currentOperation.operationId,
    });
  }
  
  // ... rest of restoration ...
}
```

---

### Phase 2: Integrate with LLM Execution (2 hours)

#### Step 2.1: Track LLM Streaming Progress

**File:** `sdk/workflow/execution/coordinators/llm-execution-coordinator.ts`

```typescript
async executeLLMWithStreaming(params: LLMExecutionParams): Promise<...> {
  const { executionId, nodeId, abortSignal } = params;
  
  // Track operation start
  const operationId = generateId();
  this.workflowExecutionEntity.state.setCurrentOperation({
    type: "LLM_STREAMING",
    operationId,
    nodeId,
    startedAt: now(),
    progress: {
      tokensGenerated: 0,
    },
    metadata: {
      profileId: params.profileId,
    },
  });
  
  try {
    // Execute LLM call with streaming
    const result = await this.llmExecutor.executeLLMCall(
      messages,
      config,
      { 
        abortSignal,
        // NEW: Progress callback
        onProgress: (chunk) => {
          // Update operation state
          this.workflowExecutionEntity.state.updateOperationProgress({
            tokensGenerated: (prev?.tokensGenerated || 0) + (chunk.tokens || 0),
          });
          
          // Accumulate partial result
          const currentPartial = this.workflowExecutionEntity.state.getCurrentOperation()?.partialResult as string || "";
          this.workflowExecutionEntity.state.updatePartialResult(currentPartial + chunk.content);
        },
      }
    );
    
    // Clear operation state on success
    this.workflowExecutionEntity.state.clearOperation();
    
    return result;
  } catch (error) {
    // If interrupted, KEEP operation state for resume
    if (isAbortError(error)) {
      logger.info("LLM streaming interrupted, preserving operation state for resume", {
        executionId,
        operationId,
      });
      // Don't clear operation state - it will be restored on resume
    } else {
      // On error, clear operation state
      this.workflowExecutionEntity.state.clearOperation();
    }
    throw error;
  }
}
```

#### Step 2.2: Resume LLM Streaming from Checkpoint

**File:** `sdk/workflow/execution/coordinators/llm-execution-coordinator.ts`

```typescript
async resumeLLMExecution(params: LLMExecutionParams): Promise<...> {
  const operationState = this.workflowExecutionEntity.state.getCurrentOperation();
  
  if (operationState?.type === "LLM_STREAMING") {
    logger.info("Resuming LLM streaming from checkpoint", {
      operationId: operationState.operationId,
      tokensGenerated: operationState.progress?.tokensGenerated,
      partialResultLength: (operationState.partialResult as string)?.length || 0,
    });
    
    // Option 1: Continue streaming (if LLM API supports it)
    // This requires LLM provider support for continuation
    const partialContent = operationState.partialResult as string || "";
    
    // Modify prompt to include partial content context
    const resumedPrompt = `${params.prompt}\n\n[Previous partial response: ${partialContent}]`;
    
    return await this.executeLLMWithStreaming({
      ...params,
      prompt: resumedPrompt,
    });
    
    // Option 2: Restart with context (fallback)
    // If continuation not supported, restart but use partial result as context
  }
  
  // No operation state, normal execution
  return await this.executeLLMWithStreaming(params);
}
```

---

### Phase 3: Integrate with Tool Execution (2 hours)

#### Step 3.1: Track Tool Execution Progress

**File:** `sdk/core/executors/tool-call-executor.ts`

```typescript
export class ToolCallExecutor {
  /**
   * Execute tool with progress tracking
   */
  async executeToolWithProgress(
    toolCall: ToolCall,
    executionId: string,
    nodeId: string,
    abortSignal?: AbortSignal,
    onProgress?: (progress: ToolProgress) => void,
  ): Promise<ToolExecutionResult> {
    const operationId = toolCall.id;
    
    // Track operation start
    const workflowExecutionEntity = this.getWorkflowExecutionEntity(executionId);
    if (workflowExecutionEntity) {
      workflowExecutionEntity.state.setCurrentOperation({
        type: "TOOL_EXECUTION",
        operationId,
        nodeId,
        startedAt: now(),
        progress: {
          itemsProcessed: 0,
          percentage: 0,
        },
        metadata: {
          toolName: toolCall.name,
          arguments: toolCall.arguments,
        },
      });
    }
    
    try {
      // Execute tool with progress callbacks
      const result = await this.executeTool(toolCall, {
        abortSignal,
        onProgress: (progress) => {
          // Update operation state
          if (workflowExecutionEntity) {
            workflowExecutionEntity.state.updateOperationProgress({
              itemsProcessed: progress.itemsProcessed,
              totalItems: progress.totalItems,
              percentage: progress.percentage,
            });
            
            // Store partial result
            workflowExecutionEntity.state.updatePartialResult(progress.partialResult);
          }
          
          // Call user-provided callback
          if (onProgress) {
            onProgress(progress);
          }
        },
      });
      
      // Clear operation state on success
      if (workflowExecutionEntity) {
        workflowExecutionEntity.state.clearOperation();
      }
      
      return result;
    } catch (error) {
      // If interrupted, preserve operation state
      if (isAbortError(error)) {
        logger.info("Tool execution interrupted, preserving state for resume", {
          executionId,
          operationId,
          toolName: toolCall.name,
        });
      } else {
        // On error, clear operation state
        if (workflowExecutionEntity) {
          workflowExecutionEntity.state.clearOperation();
        }
      }
      throw error;
    }
  }
}
```

#### Step 3.2: Resume Tool Execution

**File:** `sdk/workflow/execution/handlers/node-handlers/llm-handler.ts` (for tool calls)

```typescript
async executeToolCallsWithResume(toolCalls: ToolCall[]): Promise<...> {
  for (const toolCall of toolCalls) {
    // Check if this tool was interrupted
    const operationState = this.workflowExecutionEntity.state.getCurrentOperation();
    
    if (operationState?.type === "TOOL_EXECUTION" && 
        operationState.operationId === toolCall.id) {
      
      logger.info("Resuming interrupted tool execution", {
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        itemsProcessed: operationState.progress?.itemsProcessed,
      });
      
      // Resume tool with previous state
      const partialResult = operationState.partialResult;
      
      // Pass partial state to tool executor
      const result = await this.toolCallExecutor.executeToolWithProgress(
        toolCall,
        this.workflowExecutionEntity.id,
        this.nodeId,
        this.abortSignal,
        undefined, // onProgress
        { resumeFrom: partialResult } // NEW: Resume context
      );
      
      continue;
    }
    
    // Normal tool execution
    await this.toolCallExecutor.executeToolWithProgress(...);
  }
}
```

---

### Phase 4: Node Handler Integration (1-2 hours)

#### Step 4.1: Update Node Execution Coordinator

**File:** `sdk/workflow/execution/coordinators/node-execution-coordinator.ts`

```typescript
async executeNode(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: Node,
): Promise<NodeExecutionResult> {
  // Check for interrupted operation state
  const operationState = workflowExecutionEntity.state.getCurrentOperation();
  
  if (operationState && operationState.nodeId === node.id) {
    logger.info("Node has interrupted operation, attempting resume", {
      nodeId: node.id,
      operationType: operationState.type,
      operationId: operationState.operationId,
    });
    
    // Route to appropriate resume handler based on operation type
    switch (operationState.type) {
      case "LLM_STREAMING":
        return await this.resumeLLMNode(workflowExecutionEntity, node, operationState);
      
      case "TOOL_EXECUTION":
        return await this.resumeToolNode(workflowExecutionEntity, node, operationState);
      
      default:
        logger.warn("Unknown operation type, restarting node", {
          operationType: operationState.type,
        });
        // Clear invalid operation state
        workflowExecutionEntity.state.clearOperation();
        break;
    }
  }
  
  // Normal node execution
  return await this.executeNodeNormal(workflowExecutionEntity, node);
}

private async resumeLLMNode(
  entity: WorkflowExecutionEntity,
  node: Node,
  operationState: OperationState,
): Promise<NodeExecutionResult> {
  // Delegate to LLM coordinator with resume context
  return await this.llmCoordinator.resumeLLMExecution({
    executionId: entity.id,
    nodeId: node.id,
    prompt: /* extract from context */,
    profileId: /* extract from config */,
    // ... other params
  });
}

private async resumeToolNode(
  entity: WorkflowExecutionEntity,
  node: Node,
  operationState: OperationState,
): Promise<NodeExecutionResult> {
  // Resume tool execution with saved state
  // Implementation depends on tool type
  // ...
}
```

---

## Testing Strategy

### Unit Tests

1. **Operation State Management**
   ```typescript
   test('should track LLM streaming progress', () => {
     state.setCurrentOperation({ type: 'LLM_STREAMING', ... });
     state.updateOperationProgress({ tokensGenerated: 100 });
     expect(state.getCurrentOperation()?.progress?.tokensGenerated).toBe(100);
   });
   
   test('should serialize/deserialize operation state', () => {
     const snapshot = state.getOperationStateSnapshot();
     state.restoreOperationState(snapshot);
     expect(state.getCurrentOperation()).toEqual(snapshot);
   });
   ```

2. **Checkpoint Integration**
   ```typescript
   test('should include operation state in checkpoint', async () => {
     state.setCurrentOperation({ type: 'LLM_STREAMING', ... });
     const checkpointId = await createCheckpoint(...);
     const checkpoint = await loadCheckpoint(checkpointId);
     expect(checkpoint.snapshot.currentOperation).toBeDefined();
   });
   ```

### Integration Tests

1. **LLM Streaming Interruption & Resume**
   ```typescript
   test('should resume LLM streaming from checkpoint', async () => {
     // Start LLM call
     const llmPromise = executeLLM(...);
     
     // Interrupt at 50%
     setTimeout(() => pauseWorkflow(executionId), 1000);
     
     // Wait for pause
     await waitForPause(executionId);
     
     // Verify operation state preserved
     const checkpoint = await getLatestCheckpoint(executionId);
     expect(checkpoint.snapshot.currentOperation?.type).toBe('LLM_STREAMING');
     
     // Resume
     const result = await resumeWorkflow(executionId);
     
     // Verify completion without full restart
     expect(result.executionTime).toBeLessThan(fullRestartTime);
   });
   ```

2. **Tool Execution Interruption & Resume**
   ```typescript
   test('should resume tool execution from checkpoint', async () => {
     // Similar pattern for tool execution
   });
   ```

### End-to-End Tests

1. **Real Workflow with Long Operations**
   - Create workflow with LLM node that generates long responses
   - Pause mid-streaming
   - Resume and verify faster completion
   - Verify no duplicate content

2. **Batch Processing Tool**
   - Create tool that processes 100 items
   - Pause at item 50
   - Resume and verify continues from item 51
   - Verify final result includes all 100 items

---

## Challenges & Mitigations

### Challenge 1: LLM API Limitations
**Problem:** Most LLM APIs don't support true streaming continuation

**Mitigation:**
- Use partial result as context in resumed prompt
- Accept some re-computation (better than full restart)
- Document limitation clearly

### Challenge 2: Tool Idempotency
**Problem:** Not all tools can safely resume from partial state

**Mitigation:**
- Require tools to opt-in to resumable execution
- Provide `resumeFrom` parameter in tool interface
- Fall back to restart if resume not supported

### Challenge 3: State Complexity
**Problem:** Operation state adds complexity to checkpoint serialization

**Mitigation:**
- Keep operation state simple and serializable
- Exclude non-essential metadata
- Test serialization thoroughly

### Challenge 4: Concurrent Operations
**Problem:** Multiple operations might run in parallel (FORK nodes)

**Mitigation:**
- Track operation state per execution context
- Use fork path ID in operation metadata
- Restore correct operation based on execution path

---

## Migration Guide

### For Existing Code

**No breaking changes** - Operation state tracking is additive:

1. **WorkflowExecutionState** - New methods are optional
2. **Checkpoints** - `currentOperation` field is optional
3. **Node Handlers** - Fall back to normal execution if no operation state

### For New Development

When implementing resumable operations:

```typescript
// 1. Set operation state at start
state.setCurrentOperation({
  type: "YOUR_OPERATION_TYPE",
  operationId: generateId(),
  nodeId: currentNodeId,
  startedAt: now(),
});

// 2. Update progress periodically
state.updateOperationProgress({
  itemsProcessed: currentCount,
  percentage: (currentCount / totalCount) * 100,
});

// 3. Update partial result
state.updatePartialResult(partialData);

// 4. Clear on completion
state.clearOperation();

// 5. Preserve on interruption (don't clear if aborted)
```

---

## Success Metrics

After implementation:

1. **Resume Efficiency:** % reduction in resume time for interrupted operations
   - Target: >50% faster for operations >50% complete
   
2. **Resource Savings:** % reduction in redundant computation
   - Target: Proportional to completion percentage at interruption
   
3. **User Satisfaction:** Feedback on resume speed
   - Target: >80% positive feedback for long operations
   
4. **Adoption Rate:** % of workflows using resumable operations
   - Target: >30% within 3 months

---

## Implementation Timeline

| Phase | Tasks | Estimated Time | Dependencies |
|-------|-------|----------------|--------------|
| **Phase 1** | State tracking extensions | 2-3 hours | None |
| **Phase 2** | LLM integration | 2 hours | Phase 1 |
| **Phase 3** | Tool integration | 2 hours | Phase 1 |
| **Phase 4** | Node handler updates | 1-2 hours | Phase 2, 3 |
| **Testing** | Unit + Integration tests | 2-3 hours | All phases |
| **Total** | Complete implementation | 9-12 hours | - |

---

## Recommendation

**Implement incrementally:**

1. **Start with Phase 1** - Add state tracking infrastructure
2. **Add LLM support (Phase 2)** - Most common use case
3. **Test and validate** - Ensure stability
4. **Add tool support (Phase 3)** - Based on demand
5. **Complete integration (Phase 4)** - Polish and optimize

This approach minimizes risk and allows for early validation of the concept.

---

## Conclusion

Mid-node resume is an advanced feature that significantly improves user experience for long-running operations. While complex to implement, the architecture is well-supported by the existing checkpoint system. The key is to:

1. ✅ Track operation state granularly
2. ✅ Persist state in checkpoints
3. ✅ Restore and continue on resume
4. ✅ Handle edge cases gracefully

With careful implementation and thorough testing, this feature will make the workflow engine truly enterprise-grade.
