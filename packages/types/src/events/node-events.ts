/**
 * Node Related Event Type Definitions
 */

import type { ID, Timestamp } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Node start event type
 */
export interface NodeStartedEvent extends BaseEvent {
  type: "NODE_STARTED";
  /** Node ID */
  nodeId: ID;
  /** Node type */
  nodeType: string;
}

/**
 * Node Completion Event Type
 */
export interface NodeCompletedEvent extends BaseEvent {
  type: "NODE_COMPLETED";
  /** Node ID */
  nodeId: ID;
  /** output data */
  output: unknown;
  /** execution time */
  executionTime: Timestamp;
}

/**
 * Node failure event type
 */
export interface NodeFailedEvent extends BaseEvent {
  type: "NODE_FAILED";
  /** Node ID */
  nodeId: ID;
  /** error message */
  error: unknown;
}

/**
 * Node custom event types
 */
export interface NodeCustomEvent extends BaseEvent {
  type: "NODE_CUSTOM_EVENT";
  /** Node ID */
  nodeId: ID;
  /** Node type */
  nodeType: string;
  /** Custom Event Names */
  eventName: string;
  /** Event data */
  eventData: Record<string, unknown>;
}

/**
 * Fork node start event type
 */
export interface ForkStartedEvent extends BaseEvent {
  type: "FORK_STARTED";
  /** Node ID */
  nodeId: ID;
  /** Number of fork branches */
  branchCount: number;
}

/**
 * Fork branch start event type
 */
export interface ForkBranchStartedEvent extends BaseEvent {
  type: "FORK_BRANCH_STARTED";
  /** Node ID */
  nodeId: ID;
  /** Fork path ID */
  forkPathId: string;
  /** Branch execution ID */
  branchExecutionId: ID;
}

/**
 * Fork branch completion event type
 */
export interface ForkBranchCompletedEvent extends BaseEvent {
  type: "FORK_BRANCH_COMPLETED";
  /** Node ID */
  nodeId: ID;
  /** Fork path ID */
  forkPathId: string;
  /** Branch execution ID */
  branchExecutionId: ID;
  /** Branch execution status */
  status: string;
  /** Execution time in milliseconds */
  executionTime: number;
}

/**
 * Fork node completion event type
 */
export interface ForkCompletedEvent extends BaseEvent {
  type: "FORK_COMPLETED";
  /** Node ID */
  nodeId: ID;
  /** Total number of branches */
  totalBranches: number;
  /** Number of successful branches */
  successCount: number;
  /** Number of failed branches */
  failureCount: number;
  /** Total execution time in milliseconds */
  totalExecutionTime: number;
}

/**
 * Sync node start event type
 */
export interface NodeSyncStartedEvent extends BaseEvent {
  type: "NODE_SYNC_STARTED";
  /** Node ID */
  nodeId: ID;
  /** Source path ID - the branch to sync from */
  sourcePathId: ID;
}

/**
 * Sync node completion event type
 */
export interface NodeSyncCompletedEvent extends BaseEvent {
  type: "NODE_SYNC_COMPLETED";
  /** Node ID */
  nodeId: ID;
  /** Source path ID */
  sourcePathId: ID;
  /** Number of variables synced */
  variableCount: number;
}

/**
 * Sync node failure event type
 */
export interface NodeSyncFailedEvent extends BaseEvent {
  type: "NODE_SYNC_FAILED";
  /** Node ID */
  nodeId: ID;
  /** Source path ID */
  sourcePathId: ID;
  /** Error message */
  error: string;
}
