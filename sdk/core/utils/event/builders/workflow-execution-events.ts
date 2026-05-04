/**
 * Workflow Execution Event Builders
 * Provides builders for workflow execution lifecycle events
 */

import { now, generateId } from "@wf-agent/common-utils";
import type { WorkflowExecution, WorkflowExecutionResult } from "@wf-agent/types";
import type {
  WorkflowExecutionStartedEvent,
  WorkflowExecutionCompletedEvent,
  WorkflowExecutionFailedEvent,
  WorkflowExecutionPausedEvent,
  WorkflowExecutionResumedEvent,
  WorkflowExecutionCancelledEvent,
  WorkflowExecutionStateChangedEvent,
  WorkflowExecutionForkStartedEvent,
  WorkflowExecutionForkCompletedEvent,
  WorkflowExecutionJoinStartedEvent,
  WorkflowExecutionJoinConditionMetEvent,
  WorkflowExecutionCopyStartedEvent,
  WorkflowExecutionCopyCompletedEvent,
} from "@wf-agent/types";
import { createBuilder, createErrorBuilder } from "./common.js";
import type { WorkflowExecutionEntity } from "../../../../workflow/entities/workflow-execution-entity.js";

// =============================================================================
// Workflow Execution Lifecycle Events (built from WorkflowExecutionEntity)
// =============================================================================

/**
 * Build workflow execution started event
 */
export const buildWorkflowExecutionStartedEvent = (workflowExecutionEntity: WorkflowExecutionEntity): WorkflowExecutionStartedEvent => ({
  id: generateId(),
  type: "WORKFLOW_EXECUTION_STARTED",
  timestamp: now(),
  workflowId: workflowExecutionEntity.getWorkflowId(),
  executionId: workflowExecutionEntity.id,
  input: workflowExecutionEntity.getInput(),
});

/**
 * Build workflow execution completed event
 */
export const buildWorkflowExecutionCompletedEvent = (
  workflowExecutionEntity: WorkflowExecutionEntity,
  result: WorkflowExecutionResult,
): WorkflowExecutionCompletedEvent => ({
  id: generateId(),
  type: "WORKFLOW_EXECUTION_COMPLETED",
  timestamp: now(),
  workflowId: workflowExecutionEntity.getWorkflowId(),
  executionId: workflowExecutionEntity.id,
  output: result.output,
  executionTime: result.executionTime,
});

/**
 * Build workflow execution failed event
 */
export const buildWorkflowExecutionFailedEvent = createErrorBuilder<WorkflowExecutionFailedEvent>("WORKFLOW_EXECUTION_FAILED");

/**
 * Build workflow execution paused event
 * Enhanced with rich context information
 */
export const buildWorkflowExecutionPausedEvent = (
  workflowExecutionEntity: WorkflowExecutionEntity,
  context?: {
    nodeId?: string;
    completedNodes?: number;
    pendingToolsCancelled?: boolean;
    checkpointCreated?: boolean;
    checkpointId?: string;
  },
): WorkflowExecutionPausedEvent => ({
  id: generateId(),
  type: "WORKFLOW_EXECUTION_PAUSED",
  timestamp: now(),
  workflowId: workflowExecutionEntity.getWorkflowId(),
  executionId: workflowExecutionEntity.id,
  reason: context?.nodeId ? `Paused at node: ${context.nodeId}` : undefined,
  nodeId: context?.nodeId,
  completedNodes: context?.completedNodes,
  pendingToolsCancelled: context?.pendingToolsCancelled,
  checkpointCreated: context?.checkpointCreated,
  checkpointId: context?.checkpointId,
});

/**
 * Build workflow execution resumed event
 */
export const buildWorkflowExecutionResumedEvent = (workflowExecutionEntity: WorkflowExecutionEntity): WorkflowExecutionResumedEvent => ({
  id: generateId(),
  type: "WORKFLOW_EXECUTION_RESUMED",
  timestamp: now(),
  workflowId: workflowExecutionEntity.getWorkflowId(),
  executionId: workflowExecutionEntity.id,
});

/**
 * Build workflow execution cancelled event
 * Enhanced with rich context information
 */
export const buildWorkflowExecutionCancelledEvent = (
  workflowExecutionEntity: WorkflowExecutionEntity,
  reason?: string,
  context?: {
    nodeId?: string;
    completedNodes?: number;
    pendingToolsCancelled?: boolean;
    checkpointCreated?: boolean;
    checkpointId?: string;
    pauseDuration?: number;
    maxPauseDuration?: number;
  },
): WorkflowExecutionCancelledEvent => ({
  id: generateId(),
  type: "WORKFLOW_EXECUTION_CANCELLED",
  timestamp: now(),
  workflowId: workflowExecutionEntity.getWorkflowId(),
  executionId: workflowExecutionEntity.id,
  reason: context?.nodeId ? `${reason || "Cancelled"} at node: ${context.nodeId}` : reason,
  nodeId: context?.nodeId,
  completedNodes: context?.completedNodes,
  pendingToolsCancelled: context?.pendingToolsCancelled,
  checkpointCreated: context?.checkpointCreated,
  checkpointId: context?.checkpointId,
  pauseDuration: context?.pauseDuration,
  maxPauseDuration: context?.maxPauseDuration,
});

/**
 * Build workflow execution state changed event
 */
export const buildWorkflowExecutionStateChangedEvent = (
  workflowExecutionEntity: WorkflowExecutionEntity,
  previousStatus: string,
  newStatus: string,
): WorkflowExecutionStateChangedEvent => ({
  id: generateId(),
  type: "WORKFLOW_EXECUTION_STATE_CHANGED",
  timestamp: now(),
  workflowId: workflowExecutionEntity.getWorkflowId(),
  executionId: workflowExecutionEntity.id,
  previousStatus,
  newStatus,
});

// =============================================================================
// Fork/Join/Copy Events
// =============================================================================

/**
 * Build workflow execution fork started event
 */
export const buildWorkflowExecutionForkStartedEvent =
  createBuilder<WorkflowExecutionForkStartedEvent>("WORKFLOW_EXECUTION_FORK_STARTED");

/**
 * Build workflow execution fork completed event
 */
export const buildWorkflowExecutionForkCompletedEvent =
  createBuilder<WorkflowExecutionForkCompletedEvent>("WORKFLOW_EXECUTION_FORK_COMPLETED");

/**
 * Build workflow execution join started event
 */
export const buildWorkflowExecutionJoinStartedEvent =
  createBuilder<WorkflowExecutionJoinStartedEvent>("WORKFLOW_EXECUTION_JOIN_STARTED");

/**
 * Build workflow execution join condition met event
 */
export const buildWorkflowExecutionJoinConditionMetEvent =
  createBuilder<WorkflowExecutionJoinConditionMetEvent>("WORKFLOW_EXECUTION_JOIN_CONDITION_MET");

/**
 * Build workflow execution copy started event
 */
export const buildWorkflowExecutionCopyStartedEvent =
  createBuilder<WorkflowExecutionCopyStartedEvent>("WORKFLOW_EXECUTION_COPY_STARTED");

/**
 * Build workflow execution copy completed event
 */
export const buildWorkflowExecutionCopyCompletedEvent =
  createBuilder<WorkflowExecutionCopyCompletedEvent>("WORKFLOW_EXECUTION_COPY_COMPLETED");
