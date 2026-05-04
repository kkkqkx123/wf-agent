/**
 * Workflow Execution Related Event Type Definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Workflow Execution Start Event Type
 */
export interface WorkflowExecutionStartedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_STARTED";
  /** input data */
  input: Record<string, unknown>;
}

/**
 * Workflow Execution completion event type
 */
export interface WorkflowExecutionCompletedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_COMPLETED";
  /** output data */
  output: Record<string, unknown>;
  /** execution time */
  executionTime: number;
}

/**
 * Workflow Execution failure event type
 */
export interface WorkflowExecutionFailedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_FAILED";
  /** error message */
  error: unknown;
}

/**
 * Workflow Execution Suspension Event Type
 */
export interface WorkflowExecutionPausedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_PAUSED";
  /** Reason for suspension */
  reason?: string;
  /** Node ID where pause occurred */
  nodeId?: string;
  /** Number of completed nodes */
  completedNodes?: number;
  /** Whether pending tools were cancelled */
  pendingToolsCancelled?: boolean;
  /** Whether a checkpoint was created */
  checkpointCreated?: boolean;
  /** Checkpoint ID */
  checkpointId?: string;
}

/**
 * Workflow Execution Recovery Event Type
 */
export interface WorkflowExecutionResumedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_RESUMED";
}

/**
 * Workflow Execution cancel event type
 */
export interface WorkflowExecutionCancelledEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_CANCELLED";
  /** Reason for cancellation */
  reason?: string;
  /** Node ID where cancellation occurred */
  nodeId?: string;
  /** Number of completed nodes */
  completedNodes?: number;
  /** Whether pending tools were cancelled */
  pendingToolsCancelled?: boolean;
  /** Whether a checkpoint was created */
  checkpointCreated?: boolean;
  /** Checkpoint ID */
  checkpointId?: string;
  /** Pause duration in milliseconds (if cancelled due to pause timeout) */
  pauseDuration?: number;
  /** Maximum pause duration configured (if cancelled due to pause timeout) */
  maxPauseDuration?: number;
}

/**
 * Workflow Execution state change event type
 */
export interface WorkflowExecutionStateChangedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_STATE_CHANGED";
  /** Status before change */
  previousStatus: string;
  /** Status after change */
  newStatus: string;
}

/**
 * Workflow Execution fork start event type
 */
export interface WorkflowExecutionForkStartedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_FORK_STARTED";
  /** Parent Workflow Execution ID */
  parentExecutionId: ID;
  /** Fork Placement */
  forkConfig: Record<string, unknown>;
}

/**
 * Workflow Execution Fork Completion Event Type
 */
export interface WorkflowExecutionForkCompletedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_FORK_COMPLETED";
  /** Parent Workflow Execution ID */
  parentExecutionId: ID;
  /** Array of child execution IDs */
  childExecutionIds: ID[];
}

/**
 * Workflow Execution merge start event type
 */
export interface WorkflowExecutionJoinStartedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_JOIN_STARTED";
  /** Parent Workflow Execution ID */
  parentExecutionId: ID;
  /** Array of child execution IDs */
  childExecutionIds: ID[];
  /** merger strategy */
  joinStrategy: string;
}

/**
 * Workflow Execution Merge Condition Satisfies Event Type
 */
export interface WorkflowExecutionJoinConditionMetEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_JOIN_CONDITION_MET";
  /** Parent Workflow Execution ID */
  parentExecutionId: ID;
  /** Array of child execution IDs */
  childExecutionIds: ID[];
  /** Conditions met */
  condition: string;
}

/**
 * Workflow Execution copy start event type
 */
export interface WorkflowExecutionCopyStartedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_COPY_STARTED";
  /** Source Workflow Execution ID */
  sourceExecutionId: ID;
}

/**
 * Workflow Execution Copy Completion Event Type
 */
export interface WorkflowExecutionCopyCompletedEvent extends BaseEvent {
  type: "WORKFLOW_EXECUTION_COPY_COMPLETED";
  /** Source Workflow Execution ID */
  sourceExecutionId: ID;
  /** Copied Workflow Execution ID */
  copiedExecutionId: ID;
}
