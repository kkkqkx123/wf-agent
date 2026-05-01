/**
 * Graph Checkpoint Type Definition
 */

import type { ID, Timestamp } from "../../common.js";
import type { WorkflowExecutionStateSnapshot } from "./snapshot.js";
import type { WorkflowExecutionStatus } from "../../workflow-execution/index.js";
import type { NodeExecutionResult } from "../../workflow-execution/index.js";
import type { AnyCheckpoint } from "../base.js";

/**
 * incremental data structure
 */
export interface CheckpointDelta {
  /** New messages added */
  addedMessages?: unknown[];
  /** Message Changes (Index -> New Message) */
  modifiedMessages?: Map<number, unknown>;
  /** Deleted message index */
  deletedMessageIndices?: number[];
  /** New variables added */
  addedVariables?: unknown[];
  /** Modified variables */
  modifiedVariables?: Map<string, unknown>;
  /** Added node results */
  addedNodeResults?: Record<string, NodeExecutionResult>;
  /** Status change */
  statusChange?: {
    from: WorkflowExecutionStatus;
    to: WorkflowExecutionStatus;
  };
  /** Current Node Change */
  currentNodeChange?: {
    from: ID;
    to: ID;
  };
  /** Other status differences */
  otherChanges?: Record<string, { from: unknown; to: unknown }>;
}

/**
 * Graph Checkpoint
 * Uses AnyCheckpoint union type for strong typing
 */
export type Checkpoint = AnyCheckpoint<CheckpointDelta, WorkflowExecutionStateSnapshot> & {
  /** Associated Execution ID */
  executionId: ID;
  /** Associated Workflow ID */
  workflowId: ID;
  /** Creation timestamp */
  timestamp: Timestamp;
};
