/**
 * Thread Event Builders
 * Provides builders for thread lifecycle events
 */

import { now } from "@wf-agent/common-utils";
import type { Thread, WorkflowExecutionResult } from "@wf-agent/types";
import type {
  ThreadStartedEvent,
  ThreadCompletedEvent,
  ThreadFailedEvent,
  ThreadPausedEvent,
  ThreadResumedEvent,
  ThreadCancelledEvent,
  ThreadStateChangedEvent,
  ThreadForkStartedEvent,
  ThreadForkCompletedEvent,
  ThreadJoinStartedEvent,
  ThreadJoinConditionMetEvent,
  ThreadCopyStartedEvent,
  ThreadCopyCompletedEvent,
} from "@wf-agent/types";
import { createBuilder, createErrorBuilder } from "./common.js";
import type { ThreadEntity } from "../../../../workflow/entities/workflow-execution-entity.js";

// =============================================================================
// Thread Lifecycle Events (built from ThreadEntity)
// =============================================================================

/**
 * Build thread started event
 */
export const buildThreadStartedEvent = (threadEntity: ThreadEntity): ThreadStartedEvent => ({
  type: "THREAD_STARTED",
  timestamp: now(),
  workflowId: threadEntity.getWorkflowId(),
  threadId: workflowExecutionEntity.id,
  input: threadEntity.getInput(),
});

/**
 * Build thread completed event
 */
export const buildThreadCompletedEvent = (
  threadEntity: ThreadEntity,
  result: WorkflowExecutionResult,
): ThreadCompletedEvent => ({
  type: "THREAD_COMPLETED",
  timestamp: now(),
  workflowId: threadEntity.getWorkflowId(),
  threadId: workflowExecutionEntity.id,
  output: result.output,
  executionTime: result.executionTime,
});

/**
 * Build thread failed event
 */
export const buildThreadFailedEvent = createErrorBuilder<ThreadFailedEvent>("THREAD_FAILED");

/**
 * Build thread paused event
 */
export const buildThreadPausedEvent = (threadEntity: ThreadEntity): ThreadPausedEvent => ({
  type: "THREAD_PAUSED",
  timestamp: now(),
  workflowId: threadEntity.getWorkflowId(),
  threadId: workflowExecutionEntity.id,
});

/**
 * Build thread resumed event
 */
export const buildThreadResumedEvent = (threadEntity: ThreadEntity): ThreadResumedEvent => ({
  type: "THREAD_RESUMED",
  timestamp: now(),
  workflowId: threadEntity.getWorkflowId(),
  threadId: workflowExecutionEntity.id,
});

/**
 * Build thread cancelled event
 */
export const buildThreadCancelledEvent = (
  threadEntity: ThreadEntity,
  reason?: string,
): ThreadCancelledEvent => ({
  type: "THREAD_CANCELLED",
  timestamp: now(),
  workflowId: threadEntity.getWorkflowId(),
  threadId: workflowExecutionEntity.id,
  reason,
});

/**
 * Build thread state changed event
 */
export const buildThreadStateChangedEvent = (
  threadEntity: ThreadEntity,
  previousStatus: string,
  newStatus: string,
): ThreadStateChangedEvent => ({
  type: "THREAD_STATE_CHANGED",
  timestamp: now(),
  workflowId: threadEntity.getWorkflowId(),
  threadId: workflowExecutionEntity.id,
  previousStatus,
  newStatus,
});

// =============================================================================
// Fork/Join/Copy Events
// =============================================================================

/**
 * Build thread fork started event
 */
export const buildThreadForkStartedEvent =
  createBuilder<ThreadForkStartedEvent>("THREAD_FORK_STARTED");

/**
 * Build thread fork completed event
 */
export const buildThreadForkCompletedEvent =
  createBuilder<ThreadForkCompletedEvent>("THREAD_FORK_COMPLETED");

/**
 * Build thread join started event
 */
export const buildThreadJoinStartedEvent =
  createBuilder<ThreadJoinStartedEvent>("THREAD_JOIN_STARTED");

/**
 * Build thread join condition met event
 */
export const buildThreadJoinConditionMetEvent = createBuilder<ThreadJoinConditionMetEvent>(
  "THREAD_JOIN_CONDITION_MET",
);

/**
 * Build thread copy started event
 */
export const buildThreadCopyStartedEvent =
  createBuilder<ThreadCopyStartedEvent>("THREAD_COPY_STARTED");

/**
 * Build thread copy completed event
 */
export const buildThreadCopyCompletedEvent =
  createBuilder<ThreadCopyCompletedEvent>("THREAD_COPY_COMPLETED");
