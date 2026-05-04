# Workflow Interruption Implementation Improvement Plan

## Executive Summary

This document outlines critical improvements needed for the workflow interruption handling system. The current implementation has fundamental architectural issues including exception-based control flow (anti-pattern), lack of proper checkpoint-based resume functionality, and incomplete pending operation cancellation. These issues make the pause/resume feature unreliable for production use.

**Key Findings:**
- 🔴 **Critical:** Exception-based control flow violates established patterns
- 🔴 **Critical:** Resume re-executes entire workflow instead of restoring from checkpoint
- 🔴 **Critical:** No cancellation of pending tool calls or LLM operations
- 🟡 **Medium:** Incomplete interruption checking coverage
- 🟡 **Medium:** Limited event observability
- 🟡 **Medium:** Reactive-only checkpoint strategy

---

## Current Architecture Analysis

### Existing Components

```
Workflow Interruption System:
├── InterruptionState (sdk/core/types/interruption-state.ts)
│   ├── Manages PAUSE/STOP state
│   ├── Controls AbortController
│   └── Provides getAbortSignal()
│
├── InterruptionDetector (sdk/workflow/execution/interruption-detector.ts)
│   ├── Unified detection interface
│   ├── Checks execution abort status
│   └── Retrieves interruption type
│
├── WorkflowExecutionCoordinator (sdk/workflow/execution/coordinators/workflow-execution-coordinator.ts)
│   ├── Main execution loop
│   ├── ❌ Throws exceptions for interruptions (PROBLEM)
│   └── Delegates to node coordinator
│
├── NodeExecutionCoordinator (sdk/workflow/execution/coordinators/node-execution-coordinator.ts)
│   ├── ✅ Uses checkInterruption() correctly
│   ├── Creates checkpoints on interruption
│   └── Emits pause/cancel events
│
└── LLMExecutionCoordinator (sdk/workflow/execution/coordinators/llm-execution-coordinator.ts)
    ├── ✅ Checks interruption before/after LLM calls
    ├── ✅ Checks before tool execution
    └── Passes AbortSignal to executors
```

### Comparison with Agent Module

| Feature | Agent Module | Workflow Module | Status |
|---------|--------------|-----------------|--------|
| Control Flow Pattern | Return-value (`checkInterruption()`) | Exception throwing | ❌ Inconsistent |
| Streaming State Preservation | ✅ Preserved in checkpoints | N/A | - |
| Pending Tool Cancellation | ✅ Implemented | ❌ Missing | 🔴 Critical |
| Resume from Checkpoint | ⚠️ Partial | ❌ Not implemented | 🔴 Critical |
| Event Richness | ✅ Detailed context | ⚠️ Minimal | 🟡 Medium |
| Interruption Checking Points | Before/after LLM & tools | Only at node boundaries | 🟡 Medium |

---

## Critical Issues

### Issue 1: Exception-Based Control Flow (Anti-Pattern) 🔴

**Location:** `sdk/workflow/execution/coordinators/workflow-execution-coordinator.ts:52-68`

**Current Implementation:**
```typescript
async execute(): Promise<WorkflowExecutionResult> {
  while (true) {
    // ❌ THROWS exception for expected control flow
    if (this.interruptionManager.shouldPause()) {
      throw new WorkflowExecutionInterruptedException(
        "Workflow execution paused",
        "PAUSE",
        executionId,
        this.workflowExecutionEntity.getCurrentNodeId(),
      );
    }

    if (this.interruptionManager.shouldStop()) {
      throw new WorkflowExecutionInterruptedException(
        "Workflow execution stopped",
        "STOP",
        executionId,
        this.workflowExecutionEntity.getCurrentNodeId(),
      );
    }
    
    // ... node execution
  }
}
```

**Problems:**
1. Violates established return-value pattern used throughout codebase
2. Exception handling overhead (expensive compared to simple checks)
3. Stack trace pollution for normal control flow
4. Inconsistent with NodeExecutionCoordinator and LLMExecutionCoordinator

**Evidence of Correct Pattern:**
```typescript
// ✅ CORRECT pattern (node-execution-coordinator.ts:273)
const interruption = checkInterruption(abortSignal);
if (!shouldContinue(interruption)) {
  await this.handleInterruption(executionId, nodeId, interruptionType);
  return cancelledResult;  // Graceful return, no exception
}
```

**Impact:** 
- Performance degradation due to exception throwing
- Confusing error tracking (interruptions appear as errors)
- Architectural inconsistency across modules

---

### Issue 2: No Graceful Resume Mechanism 🔴

**Location:** `sdk/workflow/execution/coordinators/workflow-lifecycle-coordinator.ts:126-140`

**Current Implementation:**
```typescript
async resumeWorkflowExecution(executionId: string): Promise<WorkflowExecutionResult> {
  const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
  
  // 1. State transition
  await this.workflowStateTransitor.resumeWorkflowExecution(workflowExecutionEntity);
  
  // 2. Reset interrupt status
  workflowExecutionEntity.resetInterrupt();
  
  // 3. ❌ RE-EXECUTES ENTIRE WORKFLOW FROM START!
  return await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
}
```

**Problems:**
1. No checkpoint restoration - starts from beginning
2. All completed nodes are re-executed unnecessarily
3. Wasted computational resources
4. Potential side effects (duplicate API calls, database writes, etc.)
5. Lost execution progress

**What Should Happen:**
```typescript
async resumeWorkflowExecution(executionId: string): Promise<WorkflowExecutionResult> {
  const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
  
  // 1. Restore from last checkpoint
  const checkpoint = await this.restoreLastCheckpoint(executionId);
  if (checkpoint) {
    workflowExecutionEntity.setCurrentNodeId(checkpoint.nodeId);
    workflowExecutionEntity.variableStateManager.restoreFromSnapshot(checkpoint.variables);
    workflowExecutionEntity.messageHistoryManager.restoreFromSnapshot(checkpoint.messages);
    
    logger.info("Resumed from checkpoint", {
      executionId,
      nodeId: checkpoint.nodeId,
      checkpointId: checkpoint.id,
    });
  }
  
  // 2. State transition
  await this.workflowStateTransitor.resumeWorkflowExecution(workflowExecutionEntity);
  
  // 3. Reset interrupt status
  workflowExecutionEntity.resetInterrupt();
  
  // 4. Continue from restored position
  return await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
}
```

**Impact:**
- Pause/resume feature is essentially unusable for long workflows
- Users lose all progress when resuming
- Potential data corruption from duplicate executions

---

### Issue 3: No Pending Operation Cancellation 🔴

**Location:** `sdk/workflow/execution/coordinators/node-execution-coordinator.ts:273-298`

**Current Implementation:**
```typescript
if (!shouldContinue(interruption)) {
  logger.info("Node execution interrupted", {...});
  await this.handleInterruption(executionId, nodeId, interruptionType);
  
  // ❌ Just returns CANCELLED result without cleanup
  const cancelledResult: NodeExecutionResult = {
    nodeId,
    nodeType,
    status: "CANCELLED",
    // ... no cleanup of pending operations
  };
  
  workflowExecutionEntity.addNodeResult(cancelledResult);
  return cancelledResult;
}
```

**Problems:**
1. Tool calls continue executing after pause request
2. LLM streaming responses not aborted
3. Resource leaks (connections, memory)
4. Inconsistent with agent module behavior

**Agent Module Does This Correctly:**
```typescript
// ✅ Agent module cancels pending tools
for (const tc of toolCalls) {
  entity.state.recordToolCallEnd(tc.id, undefined, "Cancelled due to interruption");
}
```

**Required Implementation:**
```typescript
async handleInterruption(...) {
  // 1. Cancel pending tool calls
  await this.cancelPendingToolCalls(executionId);
  
  // 2. Abort ongoing LLM calls
  this.abortOngoingLLMCalls(executionId);
  
  // 3. Then proceed with checkpoint and events
  await this.createInterruptionCheckpoint(type);
  await this.emitInterruptionEvent(type);
}
```

**Impact:**
- Wasted resources on operations user wants to stop
- Potential unintended side effects
- Poor user experience (pause doesn't actually pause immediately)

---

### Issue 4: Inconsistent Interruption Checking Points 🟡

**Current Coverage:**
- ✅ Before each node execution (NodeExecutionCoordinator)
- ✅ Before/after LLM calls (LLMExecutionCoordinator)
- ❌ Inside long-running tool executions
- ❌ During subgraph boundary transitions
- ❌ Between hook executions (BEFORE_EXECUTE/AFTER_EXECUTE)

**Impact:**
Delayed response to pause requests during long operations, creating poor UX.

---

### Issue 5: Poor Observability & Event Emission 🟡

**Current Events:**
```typescript
// Minimal information
{
  type: "WORKFLOW_EXECUTION_PAUSED",
  executionId: "...",
  timestamp: 123456
}
```

**Should Include:**
```typescript
// Rich context (like agent module)
{
  type: "WORKFLOW_EXECUTION_PAUSED",
  executionId: "...",
  nodeId: "current-node-id",
  completedNodes: 5,
  totalNodes: 10,
  currentVariables: { ... },
  pendingOperations: ["tool-call-1", "llm-stream"],
  checkpointCreated: true,
  checkpointId: "chk_123",
  pauseDuration: null,  // Will be set on resume
}
```

**Impact:**
Difficult to debug interruption issues and monitor workflow state.

---

### Issue 6: Checkpoint Creation Only on Interruption 🟡

**Current Strategy:**
- Checkpoints created ONLY when interruption occurs
- No periodic progress checkpoints

**Problems:**
1. If workflow crashes before interruption, all progress lost
2. Resume can't find checkpoint (none exists until interruption)
3. Inconsistent with robust checkpoint strategy

**Recommended Strategy:**
- Create checkpoints at every node completion (progress checkpoints)
- Create special checkpoint on interruption (interruption checkpoint)
- Enable true resume-from-anywhere functionality

---

### Issue 7: No Timeout for Paused State 🟢

**Problem:** Workflows remain paused indefinitely with no timeout mechanism.

**Risks:**
- Resource leaks (held connections, memory)
- Stale state (variables, messages become outdated)
- No automatic cleanup

**Recommendation:** Add configurable timeout with warning events.

---

## Improvement Plan

### Phase 1: Fix Critical Architectural Issues (Immediate)

#### Task 1.1: Replace Exception-Based Control Flow
**Priority:** 🔴 Critical  
**Effort:** 2-3 hours  
**Files:** `workflow-execution-coordinator.ts`

**Implementation:**
```typescript
// File: sdk/workflow/execution/coordinators/workflow-execution-coordinator.ts

import { checkInterruption, shouldContinue, getInterruptionType } from "@wf-agent/common-utils";

export class WorkflowExecutionCoordinator {
  async execute(): Promise<WorkflowExecutionResult> {
    const executionId = this.workflowExecutionEntity.id;
    const startTime = this.workflowExecutionEntity.getStartTime();

    while (true) {
      // ✅ Use return-value pattern instead of throwing exceptions
      const interruption = checkInterruption(this.interruptionManager.getAbortSignal());
      
      if (!shouldContinue(interruption)) {
        logger.info("Workflow execution interrupted", {
          executionId,
          interruptionType: interruption.type,
          currentNodeId: this.workflowExecutionEntity.getCurrentNodeId(),
        });
        
        // Handle interruption gracefully
        return await this.handleInterruptionGracefully(interruption);
      }

      // Get and execute current node
      const currentNodeId = this.workflowExecutionEntity.getCurrentNodeId();
      if (!currentNodeId) {
        break;
      }

      const graphNode = this.navigator.getGraph().getNode(currentNodeId);
      if (!graphNode) {
        break;
      }

      const currentNode = graphNode.originalNode || this.createDefaultNode(graphNode);
      
      // Execute node (NodeExecutionCoordinator handles its own interruption checks)
      const result = await this.nodeExecutionCoordinator.executeNode(
        this.workflowExecutionEntity,
        currentNode,
      );

      this.workflowExecutionEntity.addNodeResult(result);

      // Move to next node or exit
      if (result.status === "COMPLETED") {
        const nextNode = this.navigator.getNextNode(currentNodeId);
        if (nextNode && nextNode.nextNodeId) {
          this.workflowExecutionEntity.setCurrentNodeId(nextNode.nextNodeId);
        } else {
          break;
        }
      } else {
        break;
      }
    }

    // Build successful execution result
    return this.buildSuccessResult();
  }

  private async handleInterruptionGracefully(
    interruption: InterruptionCheckResult
  ): Promise<WorkflowExecutionResult> {
    const type = interruption.type === "paused" ? "PAUSE" : "STOP";
    const currentNodeId = this.workflowExecutionEntity.getCurrentNodeId();
    
    // Delegate to node coordinator for checkpoint creation and event emission
    if (currentNodeId) {
      await this.nodeExecutionCoordinator.handleInterruption(
        this.workflowExecutionEntity.id,
        currentNodeId,
        type,
      );
    }

    // Update status
    this.workflowExecutionEntity.setStatus(type === "PAUSE" ? "PAUSED" : "CANCELLED");

    // Build interrupted result
    return {
      executionId: this.workflowExecutionEntity.id,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() || Date.now()),
      nodeResults: this.workflowExecutionEntity.getNodeResults(),
      metadata: {
        status: type === "PAUSE" ? "PAUSED" : "CANCELLED",
        startTime: this.workflowExecutionEntity.getStartTime() || Date.now(),
        endTime: Date.now(),
        executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() || Date.now()),
        nodeCount: this.workflowExecutionEntity.getNodeResults().length,
        errorCount: this.workflowExecutionEntity.getErrors().length,
        interruptionType: type,
        interruptedAtNodeId: currentNodeId,
      },
    };
  }

  private buildSuccessResult(): WorkflowExecutionResult {
    const endTime = this.workflowExecutionEntity.getEndTime() || Date.now();
    const startTime = this.workflowExecutionEntity.getStartTime() || Date.now();
    
    return {
      executionId: this.workflowExecutionEntity.id,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime: endTime - startTime,
      nodeResults: this.workflowExecutionEntity.getNodeResults(),
      metadata: {
        status: this.workflowExecutionEntity.getStatus(),
        startTime,
        endTime,
        executionTime: endTime - startTime,
        nodeCount: this.workflowExecutionEntity.getNodeResults().length,
        errorCount: this.workflowExecutionEntity.getErrors().length,
      },
    };
  }

  private createDefaultNode(graphNode: any): Node {
    return {
      id: graphNode.id,
      type: graphNode.type,
      name: graphNode.name,
      config: {},
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    } as Node;
  }
}
```

**Testing:**
- Verify pause request is detected within one node execution
- Verify no exceptions are thrown for normal interruptions
- Verify correct result structure is returned

---

#### Task 1.2: Implement Checkpoint-Based Resume
**Priority:** 🔴 Critical  
**Effort:** 4-6 hours  
**Files:** `workflow-lifecycle-coordinator.ts`, add checkpoint restoration utilities

**Implementation:**
```typescript
// File: sdk/workflow/execution/coordinators/workflow-lifecycle-coordinator.ts

import { restoreWorkflowFromCheckpoint } from "../utils/checkpoint-restoration.js";

export class WorkflowLifecycleCoordinator {
  async resumeWorkflowExecution(executionId: string): Promise<WorkflowExecutionResult> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(`WorkflowExecutionEntity not found`, executionId);
    }

    logger.info("Resuming workflow execution", { executionId });

    // 1. Restore from last checkpoint
    const restored = await restoreWorkflowFromCheckpoint(
      executionId,
      workflowExecutionEntity,
      this.workflowExecutionRegistry,
    );
    
    if (restored) {
      logger.info("Workflow restored from checkpoint", {
        executionId,
        restoredNodeId: workflowExecutionEntity.getCurrentNodeId(),
        checkpointId: restored.checkpointId,
      });
    } else {
      logger.warn("No checkpoint found, resuming from start", { executionId });
    }

    // 2. State transition
    await this.workflowStateTransitor.resumeWorkflowExecution(workflowExecutionEntity);

    // 3. Reset interrupt status (including AbortController)
    workflowExecutionEntity.resetInterrupt();

    // 4. Continue execution from restored position
    return await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
  }
}
```

**New Utility File:** `sdk/workflow/execution/utils/checkpoint-restoration.ts`
```typescript
/**
 * Checkpoint Restoration Utilities
 * Handles restoring workflow state from checkpoints
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { CheckpointDependencies } from "../../checkpoint/utils/checkpoint-utils.js";
import { getLatestCheckpoint } from "../../checkpoint/utils/checkpoint-utils.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "checkpoint-restoration" });

export interface RestorationResult {
  success: boolean;
  checkpointId?: string;
  restoredNodeId?: string;
  error?: Error;
}

/**
 * Restore workflow execution from latest checkpoint
 */
export async function restoreWorkflowFromCheckpoint(
  executionId: string,
  workflowExecutionEntity: WorkflowExecutionEntity,
  registry: WorkflowExecutionRegistry,
): Promise<RestorationResult> {
  try {
    // Get checkpoint dependencies from container
    const container = await import("../../../core/di/index.js").then(m => m.getContainer());
    const Identifiers = await import("../../../core/di/service-identifiers.js");
    const checkpointDeps = {
      workflowExecutionRegistry: registry,
      checkpointStateManager: container.get(Identifiers.CheckpointStateManager),
      workflowRegistry: container.get(Identifiers.WorkflowRegistry),
      workflowGraphRegistry: container.get(Identifiers.WorkflowGraphRegistry),
    } as CheckpointDependencies;

    // Get latest checkpoint for this execution
    const checkpoint = await getLatestCheckpoint(executionId, checkpointDeps);
    
    if (!checkpoint) {
      logger.debug("No checkpoint found for execution", { executionId });
      return { success: false };
    }

    logger.info("Restoring from checkpoint", {
      executionId,
      checkpointId: checkpoint.id,
      nodeId: checkpoint.nodeId,
    });

    // Restore node position
    if (checkpoint.nodeId) {
      workflowExecutionEntity.setCurrentNodeId(checkpoint.nodeId);
    }

    // Restore variable state if available
    if (checkpoint.metadata?.customFields?.variables) {
      const variables = checkpoint.metadata.customFields.variables as Record<string, unknown>;
      workflowExecutionEntity.variableStateManager.restoreFromSnapshot({
        variables: new Map(Object.entries(variables)),
      });
    }

    // Restore message history if available
    if (checkpoint.metadata?.customFields?.messages) {
      const messages = checkpoint.metadata.customFields.messages as any[];
      workflowExecutionEntity.messageHistoryManager.restoreFromSnapshot(messages);
    }

    return {
      success: true,
      checkpointId: checkpoint.id,
      restoredNodeId: checkpoint.nodeId,
    };
  } catch (error) {
    logger.error("Failed to restore from checkpoint", {
      executionId,
      error: error instanceof Error ? error.message : String(error),
    });
    
    return {
      success: false,
      error: error instanceof Error ? error : new Error(String(error)),
    };
  }
}
```

**Testing:**
- Create workflow with multiple nodes
- Pause after completing some nodes
- Verify resume starts from paused node, not beginning
- Verify variable state is restored correctly
- Verify message history is preserved

---

#### Task 1.3: Add Pending Operation Cancellation
**Priority:** 🔴 Critical  
**Effort:** 3-4 hours  
**Files:** `node-execution-coordinator.ts`, add cancellation utilities

**Implementation:**
```typescript
// File: sdk/workflow/execution/coordinators/node-execution-coordinator.ts

export class NodeExecutionCoordinator {
  async handleInterruption(
    workflowExecutionId: string,
    nodeId: string,
    type: "PAUSE" | "STOP",
  ): Promise<void> {
    logger.info("Handling interruption", { 
      executionId: workflowExecutionId, 
      nodeId, 
      type 
    });

    if (!this.workflowExecutionRegistry) {
      logger.debug("WorkflowExecutionRegistry not available, skipping interruption handling");
      return;
    }

    const workflowExecutionContext = this.workflowExecutionRegistry.get(workflowExecutionId);
    if (!workflowExecutionContext) {
      logger.warn("WorkflowExecutionContext not found for interruption");
      return;
    }

    // 1. Cancel pending tool calls
    await this.cancelPendingToolCalls(workflowExecutionId, nodeId);

    // 2. Abort ongoing LLM calls (signal already triggered via InterruptionState)
    logger.debug("LLM calls will be aborted via AbortSignal");

    // 3. Create interruption checkpoint
    if (this.checkpointDependencies) {
      try {
        await createCheckpoint(
          {
            workflowExecutionId,
            nodeId,
            description: `Workflow execution ${type.toLowerCase()} at node: ${nodeId}`,
            metadata: {
              customFields: {
                interruptionType: type,
                interruptedAt: now(),
                pendingToolsCancelled: true,
              },
            },
          },
          this.checkpointDependencies,
        );
        logger.debug("Interruption checkpoint created");
      } catch (error) {
        await handleErrorWithContext(
          this.eventManager,
          getErrorOrNew(error) as SDKError,
          workflowExecutionContext,
          "CREATE_INTERRUPTION_CHECKPOINT",
        );
        throw error;
      }
    }

    // 4. Trigger corresponding event with rich context
    if (type === "PAUSE") {
      workflowExecutionContext.setStatus("PAUSED");
      const pausedEvent = buildWorkflowExecutionPausedEvent({
        executionId: workflowExecutionContext.id,
        nodeId,
        completedNodes: workflowExecutionContext.getNodeResults().length,
        pendingToolsCancelled: true,
        checkpointCreated: true,
      });
      await emit(this.eventManager, pausedEvent);
      logger.info("Workflow execution paused event emitted");
    } else if (type === "STOP") {
      workflowExecutionContext.setStatus("CANCELLED");
      workflowExecutionContext.state.cancel();
      const cancelledEvent = buildWorkflowExecutionCancelledEvent(
        workflowExecutionContext,
        "user_requested",
        {
          nodeId,
          completedNodes: workflowExecutionContext.getNodeResults().length,
          pendingToolsCancelled: true,
        }
      );
      await emit(this.eventManager, cancelledEvent);
      logger.info("Workflow execution cancelled event emitted");
    }
  }

  /**
   * Cancel pending tool calls
   */
  private async cancelPendingToolCalls(executionId: string, nodeId: string): Promise<void> {
    // Get tool call executor from context factory
    const toolCallExecutor = this.handlerContextFactory.getToolCallExecutor();
    if (!toolCallExecutor) {
      logger.debug("ToolCallExecutor not available, skipping tool cancellation");
      return;
    }

    // Cancel any pending tool calls for this execution
    try {
      await toolCallExecutor.cancelPendingCalls(executionId);
      logger.info("Pending tool calls cancelled", { executionId, nodeId });
    } catch (error) {
      logger.warn("Failed to cancel pending tool calls", {
        executionId,
        nodeId,
        error: error instanceof Error ? error.message : String(error),
      });
      // Don't throw - interruption should still proceed
    }
  }
}
```

**Add to ToolCallExecutor:**
```typescript
// File: sdk/core/executors/tool-call-executor.ts

export class ToolCallExecutor {
  private pendingCalls: Map<string, Set<string>> = new Map(); // executionId -> toolCallIds

  /**
   * Cancel all pending tool calls for an execution
   */
  async cancelPendingCalls(executionId: string): Promise<void> {
    const calls = this.pendingCalls.get(executionId);
    if (!calls || calls.size === 0) {
      return;
    }

    logger.info("Cancelling pending tool calls", {
      executionId,
      callCount: calls.size,
    });

    // Mark all as cancelled
    for (const callId of calls) {
      // Update tool call status in conversation/session
      // This depends on your tool call tracking implementation
      this.markToolCallCancelled(callId, executionId);
    }

    // Clear pending calls
    this.pendingCalls.delete(executionId);
  }

  private markToolCallCancelled(callId: string, executionId: string): void {
    // Implementation depends on how you track tool calls
    // Could update conversation history, emit event, etc.
    logger.debug("Tool call marked as cancelled", { callId, executionId });
  }
}
```

**Testing:**
- Start workflow with tool-calling node
- Request pause during tool execution
- Verify tool call is cancelled (not completed)
- Verify no resource leaks

---

### Phase 2: Enhance Observability and Coverage (Short-term)

#### Task 2.1: Add More Interruption Checking Points
**Priority:** 🟡 Medium  
**Effort:** 2-3 hours  
**Files:** Various coordinators and handlers

**Locations to Add Checks:**
1. Inside tool execution handlers
2. During subgraph boundary transitions
3. Between BEFORE_EXECUTE and AFTER_EXECUTE hooks
4. During variable state updates

**Example:**
```typescript
// In subgraph-handler.ts
export async function enterSubgraph(...) {
  // Check interruption before entering
  const interruption = checkInterruption(abortSignal);
  if (!shouldContinue(interruption)) {
    logger.info("Subgraph entry interrupted");
    return handleInterruption(...);
  }
  
  // ... subgraph entry logic
}
```

---

#### Task 2.2: Enhance Event Context
**Priority:** 🟡 Medium  
**Effort:** 2 hours  
**Files:** Event builder utilities

**Update Event Builders:**
```typescript
// File: sdk/workflow/execution/utils/event/builders.ts

export function buildWorkflowExecutionPausedEvent(
  context: {
    executionId: string;
    nodeId?: string;
    completedNodes?: number;
    pendingToolsCancelled?: boolean;
    checkpointCreated?: boolean;
    [key: string]: any;
  }
): WorkflowExecutionPausedEvent {
  return {
    id: generateId(),
    type: "WORKFLOW_EXECUTION_PAUSED",
    timestamp: now(),
    executionId: context.executionId,
    nodeId: context.nodeId,
    completedNodes: context.completedNodes || 0,
    pendingToolsCancelled: context.pendingToolsCancelled || false,
    checkpointCreated: context.checkpointCreated || false,
    // ... other fields
  };
}
```

---

#### Task 2.3: Add Periodic Progress Checkpoints
**Priority:** 🟡 Medium  
**Effort:** 2-3 hours  
**Files:** `node-execution-coordinator.ts`

**Implementation:**
```typescript
// In executeNode method, after successful completion
async executeNode(...): Promise<NodeExecutionResult> {
  // ... existing execution logic
  
  // After successful completion, create progress checkpoint
  if (this.checkpointDependencies && nodeResult.status === "COMPLETED") {
    try {
      await createCheckpoint(
        {
          workflowExecutionId: workflowExecutionEntity.id,
          nodeId: node.id,
          description: `Progress checkpoint after node: ${node.name}`,
          metadata: {
            customFields: {
              checkpointType: "PROGRESS",
              completedAt: now(),
              nodeCount: workflowExecutionEntity.getNodeResults().length,
              variables: workflowExecutionEntity.getAllVariables(),
            },
          },
        },
        this.checkpointDependencies,
      );
      logger.debug("Progress checkpoint created");
    } catch (error) {
      logger.warn("Failed to create progress checkpoint", { error });
      // Don't fail node execution due to checkpoint failure
    }
  }
  
  return nodeResult;
}
```

---

### Phase 3: Advanced Features (Long-term)

#### Task 3.1: Add Pause Timeout
**Priority:** 🟢 Low  
**Effort:** 3-4 hours  

**Implementation:**
- Add configurable timeout to workflow configuration
- Emit warning event when approaching timeout
- Auto-cancel if timeout exceeded
- Allow extension requests

---

#### Task 3.2: Implement Mid-Node Resume
**Priority:** 🟢 Low  
**Effort:** 6-8 hours  

**Description:** Allow resuming from middle of long-running node operations (e.g., during tool execution, LLM streaming).

**Requirements:**
- Save operation state in checkpoints
- Implement operation-specific resume logic
- Handle partial results gracefully

---

## Implementation Timeline

| Phase | Tasks | Estimated Time | Priority |
|-------|-------|----------------|----------|
| **Phase 1** | Task 1.1, 1.2, 1.3 | 9-13 hours | 🔴 Critical |
| **Phase 2** | Task 2.1, 2.2, 2.3 | 6-8 hours | 🟡 Medium |
| **Phase 3** | Task 3.1, 3.2 | 9-12 hours | 🟢 Low |
| **Total** | All tasks | 24-33 hours | - |

**Recommended Approach:**
1. Complete Phase 1 immediately (critical for usability)
2. Test thoroughly before proceeding
3. Implement Phase 2 in next sprint
4. Consider Phase 3 based on user feedback

---

## Testing Strategy

### Unit Tests
- Test interruption detection at various points
- Test checkpoint creation and restoration
- Test pending operation cancellation
- Test event emission with rich context

### Integration Tests
- Test full pause/resume cycle with multi-node workflow
- Test interruption during tool execution
- Test interruption during LLM calls
- Test resume from various checkpoint types

### End-to-End Tests
- Real workflow with actual tools and LLM calls
- Verify no duplicate executions on resume
- Verify resource cleanup on cancellation
- Verify state consistency after resume

---

## Migration Guide

### For Existing Code

**No breaking changes** - The improvements maintain backward compatibility:

1. Exception-based → Return-value: Internal change only, external API unchanged
2. Resume enhancement: Works with existing pause/resume API
3. Event enrichment: Adds optional fields, doesn't remove existing ones

### For New Code

When implementing new interruption-aware components:

```typescript
// ✅ DO: Use return-value pattern
const interruption = checkInterruption(signal);
if (!shouldContinue(interruption)) {
  return handleInterruption(interruption);
}

// ❌ DON'T: Throw exceptions for expected interruptions
if (shouldPause()) {
  throw new InterruptedException(...);  // Anti-pattern
}
```

---

## Success Metrics

After implementation, measure:

1. **Resume Accuracy:** % of resumes that correctly restore state (target: 100%)
2. **Interruption Latency:** Time from pause request to actual pause (target: <1 node execution)
3. **Resource Cleanup:** % of pending operations properly cancelled (target: 100%)
4. **User Satisfaction:** Feedback on pause/resume reliability (target: >90% positive)
5. **Error Rate:** Exceptions thrown for normal interruptions (target: 0)

---

## Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Checkpoint restoration fails | High | Medium | Fallback to restart with warning |
| Tool cancellation incomplete | Medium | Low | Add timeout-based cleanup |
| Performance overhead from frequent checkpoints | Low | Medium | Make checkpoint frequency configurable |
| Breaking existing integrations | High | Low | Maintain backward compatibility, thorough testing |

---

## Conclusion

The workflow interruption system requires significant improvements to match the robustness of the agent module and provide reliable pause/resume functionality. The three critical issues (exception-based control flow, lack of checkpoint-based resume, and missing operation cancellation) must be addressed immediately to make the feature production-ready.

**Next Steps:**
1. Review and approve this plan
2. Assign developers to Phase 1 tasks
3. Set up test environment for validation
4. Begin implementation with Task 1.1

**Expected Outcome:**
After completing all phases, the workflow interruption system will:
- ✅ Use consistent return-value pattern throughout
- ✅ Support reliable pause/resume from any point
- ✅ Properly cancel all pending operations
- ✅ Provide rich observability via events
- ✅ Maintain state across interruptions via checkpoints
- ✅ Match or exceed agent module capabilities
