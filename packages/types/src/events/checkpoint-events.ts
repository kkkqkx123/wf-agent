/**
 * Checkpoint Related Event Type Definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Checkpoint creation event type
 */
export interface CheckpointCreatedEvent extends BaseEvent {
  type: "CHECKPOINT_CREATED";
  /** Checkpoint ID */
  checkpointId: ID;
  /** Checkpoint Description */
  description?: string;
}

/**
 * Checkpoint recovery event type
 */
export interface CheckpointRestoredEvent extends BaseEvent {
  type: "CHECKPOINT_RESTORED";
  /** Checkpoint ID */
  checkpointId: ID;
  /** Recovered thread ID */
  threadId: ID;
  /** Checkpoint Description */
  description?: string;
}

/**
 * Checkpoint Deletion Event Type
 */
export interface CheckpointDeletedEvent extends BaseEvent {
  type: "CHECKPOINT_DELETED";
  /** Checkpoint ID */
  checkpointId: ID;
  /** Reason for deletion */
  reason?: "manual" | "cleanup" | "policy";
}

/**
 * Checkpoint failure event type
 */
export interface CheckpointFailedEvent extends BaseEvent {
  type: "CHECKPOINT_FAILED";
  /** Checkpoint ID (if generated) */
  checkpointId?: ID;
  /** Types of failed operations */
  operation: "create" | "restore" | "delete";
  /** error message */
  error: string;
}
