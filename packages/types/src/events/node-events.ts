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
