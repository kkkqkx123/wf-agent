/**
 * Checkpoint Message Types
 *
 * Messages related to checkpoint creation, restoration, and management.
 */

/**
 * Checkpoint Message Type
 */
export enum CheckpointMessageType {
  /** Checkpoint created */
  CREATE = "checkpoint.create",

  /** Checkpoint restored */
  RESTORE = "checkpoint.restore",

  /** Checkpoint deleted */
  DELETE = "checkpoint.delete",

  /** Checkpoint failed */
  FAIL = "checkpoint.fail",
}

/**
 * Checkpoint Create Data
 */
export interface CheckpointCreateData {
  /** Checkpoint ID */
  checkpointId: string;

  /** Entity type */
  entityType: "workflowExecution" | "agent";

  /** Entity ID */
  entityId: string;

  /** Checkpoint name (optional) */
  name?: string;

  /** Checkpoint path */
  path: string;

  /** Checkpoint metadata */
  metadata?: {
    /** Iteration number (for agent) */
    iteration?: number;

    /** Node ID (for workflow execution) */
    nodeId?: string;

    /** Custom metadata */
    custom?: Record<string, unknown>;
  };
}

/**
 * Checkpoint Restore Data
 */
export interface CheckpointRestoreData {
  /** Checkpoint ID */
  checkpointId: string;

  /** Entity type */
  entityType: "workflowExecution" | "agent";

  /** Entity ID */
  entityId: string;

  /** Checkpoint path */
  path: string;

  /** Restore options */
  options?: {
    /** Whether to restore variables */
    restoreVariables?: boolean;

    /** Whether to restore messages */
    restoreMessages?: boolean;

    /** Whether to restore state */
    restoreState?: boolean;
  };
}

/**
 * Checkpoint Delete Data
 */
export interface CheckpointDeleteData {
  /** Checkpoint ID */
  checkpointId: string;

  /** Entity type */
  entityType: "workflowExecution" | "agent";

  /** Entity ID */
  entityId: string;

  /** Deletion reason */
  reason: "explicit" | "cleanup" | "expired";
}

/**
 * Checkpoint Fail Data
 */
export interface CheckpointFailData {
  /** Checkpoint ID (if available) */
  checkpointId?: string;

  /** Entity type */
  entityType: "workflowExecution" | "agent";

  /** Entity ID */
  entityId: string;

  /** Operation that failed */
  operation: "create" | "restore" | "delete";

  /** Error message */
  error: string;

  /** Error code */
  code?: string;
}
