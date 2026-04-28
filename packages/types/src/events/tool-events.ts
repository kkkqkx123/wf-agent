/**
 * Tool Related Event Type Definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Tool call start event type
 */
export interface ToolCallStartedEvent extends BaseEvent {
  type: "TOOL_CALL_STARTED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool call task ID (for tracking individual tool calls) */
  taskId?: string;
  /** Batch ID (identification of a batch of parallel tool calls) */
  batchId?: string;
  /** Tool name */
  toolName?: string;
  /** Tool parameters */
  toolArguments: string;
}

/**
 * Tool call completion event type
 */
export interface ToolCallCompletedEvent extends BaseEvent {
  type: "TOOL_CALL_COMPLETED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool call task ID (for tracking individual tool calls) */
  taskId?: string;
  /** Batch ID (identification of a batch of parallel tool calls) */
  batchId?: string;
  /** Tool name */
  toolName?: string;
  /** Tool results */
  toolResult: unknown;
  /** execution time */
  executionTime: number;
}

/**
 * Tool Call Failure Event Type
 */
export interface ToolCallFailedEvent extends BaseEvent {
  type: "TOOL_CALL_FAILED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool call task ID (for tracking individual tool calls) */
  taskId?: string;
  /** Batch ID (identification of a batch of parallel tool calls) */
  batchId?: string;
  /** Tool name */
  toolName?: string;
  /** error message */
  error: string;
}

/**
 * Tool to add event types
 */
export interface ToolAddedEvent extends BaseEvent {
  type: "TOOL_ADDED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID List */
  toolIds: ID[];
  /** scope (computing) */
  scope: "GLOBAL" | "THREAD" | "LOCAL";
  /** Number of tools successfully added */
  addedCount: number;
  /** Number of tools skipped */
  skippedCount: number;
}
