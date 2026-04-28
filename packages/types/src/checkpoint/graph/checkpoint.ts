/**
 * Graph Checkpoint Type Definition
 */

import type { ID, Timestamp } from "../../common.js";
import type { ThreadStateSnapshot } from "./snapshot.js";
import type { WorkflowExecutionStatus } from "../../thread/index.js";
import type { NodeExecutionResult } from "../../thread/index.js";
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
  extends BaseCheckpoint<CheckpointDelta, ThreadStateSnapshot> {
  /** Associated Thread ID */
  threadId: ID;
  /** Associated Workflow ID */
  workflowId: ID;
  /** Creation timestamp */
  timestamp: Timestamp;
  /** Thread state snapshot (full checkpoint usage, backwards compatible) */
  threadState?: ThreadStateSnapshot;
}
