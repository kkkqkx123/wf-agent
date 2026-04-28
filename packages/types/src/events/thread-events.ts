/**
 * Thread Related Event Type Definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Thread Start Event Type
 */
export interface ThreadStartedEvent extends BaseEvent {
  type: "THREAD_STARTED";
  /** input data */
  input: Record<string, unknown>;
}

/**
 * Thread completion event type
 */
export interface ThreadCompletedEvent extends BaseEvent {
  type: "THREAD_COMPLETED";
  /** output data */
  output: Record<string, unknown>;
  /** execution time */
  executionTime: number;
}

/**
 * Thread failure event type
 */
export interface ThreadFailedEvent extends BaseEvent {
  type: "THREAD_FAILED";
  /** error message */
  error: unknown;
}

/**
 * Thread Suspension Event Type
 */
export interface ThreadPausedEvent extends BaseEvent {
  type: "THREAD_PAUSED";
  /** Reason for suspension */
  reason?: string;
}

/**
 * Thread Recovery Event Type
 */
export interface ThreadResumedEvent extends BaseEvent {
  type: "THREAD_RESUMED";
}

/**
 * Thread cancel event type
 */
export interface ThreadCancelledEvent extends BaseEvent {
  type: "THREAD_CANCELLED";
  /** Reason for cancellation */
  reason?: string;
}

/**
 * Thread state change event type
 */
export interface ThreadStateChangedEvent extends BaseEvent {
  type: "THREAD_STATE_CHANGED";
  /** Status before change */
  previousStatus: string;
  /** Status after change */
  newStatus: string;
}

/**
 * Thread fork start event type
 */
export interface ThreadForkStartedEvent extends BaseEvent {
  type: "THREAD_FORK_STARTED";
  /** Parent Thread ID */
  parentThreadId: ID;
  /** Fork Placement */
  forkConfig: Record<string, unknown>;
}

/**
 * Thread Fork Completion Event Type
 */
export interface ThreadForkCompletedEvent extends BaseEvent {
  type: "THREAD_FORK_COMPLETED";
  /** Parent Thread ID */
  parentThreadId: ID;
  /** Array of subthread IDs */
  childThreadIds: ID[];
}

/**
 * Thread merge start event type
 */
export interface ThreadJoinStartedEvent extends BaseEvent {
  type: "THREAD_JOIN_STARTED";
  /** Parent Thread ID */
  parentThreadId: ID;
  /** Array of subthread IDs */
  childThreadIds: ID[];
  /** merger strategy */
  joinStrategy: string;
}

/**
 * Thread Merge Condition Satisfies Event Type
 */
export interface ThreadJoinConditionMetEvent extends BaseEvent {
  type: "THREAD_JOIN_CONDITION_MET";
  /** Parent Thread ID */
  parentThreadId: ID;
  /** Array of subthread IDs */
  childThreadIds: ID[];
  /** Conditions met */
  condition: string;
}

/**
 * Thread copy start event type
 */
export interface ThreadCopyStartedEvent extends BaseEvent {
  type: "THREAD_COPY_STARTED";
  /** Source Thread ID */
  sourceThreadId: ID;
}

/**
 * Thread Copy Completion Event Type
 */
export interface ThreadCopyCompletedEvent extends BaseEvent {
  type: "THREAD_COPY_COMPLETED";
  /** Source Thread ID */
  sourceThreadId: ID;
  /** Copy Thread ID */
  copiedThreadId: ID;
}
