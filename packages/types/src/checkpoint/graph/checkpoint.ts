/**
 * Graph Checkpoint Type Definition
 */

import type { ID, Timestamp } from "../../common.js";
import type { WorkflowExecutionStateSnapshot } from "./snapshot.js";
import type { WorkflowExecutionStatus } from "../../workflow-execution/index.js";
import type { NodeExecutionResult } from "../../workflow-execution/index.js";
import type { BaseCheckpoint } from "../base.js";

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
 * Extends BaseCheckpoint with Graph-specific fields
 */
export interface Checkpoint
  extends BaseCheckpoint<CheckpointDelta, WorkflowExecutionStateSnapshot> {
  /** Associated Execution ID */
  executionId: ID;
  /** Associated Workflow ID */
  workflowId: ID;
  /** Creation timestamp */
  timestamp: Timestamp;
  /** Execution state snapshot (full checkpoint usage) */
  executionState?: WorkflowExecutionStateSnapshot;
}
