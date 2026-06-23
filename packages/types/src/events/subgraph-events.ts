/**
 * Subgraph Related Event Type Definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Subgraph start event type
 */
export interface SubgraphStartedEvent extends BaseEvent {
  type: "SUBGRAPH_STARTED";
  /** Subworkflow ID */
  subgraphId: ID;
  /** Parent workflow ID */
  parentWorkflowId: ID;
  /** input data */
  input: Record<string, unknown>;
}

/**
 * Subgraph completion event type
 */
export interface SubgraphCompletedEvent extends BaseEvent {
  type: "SUBGRAPH_COMPLETED";
  /** Subworkflow ID */
  subgraphId: ID;
  /** output data */
  output: Record<string, unknown>;
  /** execution time */
  executionTime: number;
}

/**
 * Trigger sub workflow start event type
 */
export interface TriggeredSubgraphStartedEvent extends BaseEvent {
  type: "TRIGGERED_SUBGRAPH_STARTED";
  /** Subworkflow ID */
  subgraphId: ID;
  /** Trigger ID */
  triggerId: ID;
  /** input data */
  input: Record<string, unknown>;
}

/**
 * Trigger sub workflow completion event type
 */
export interface TriggeredSubgraphCompletedEvent extends BaseEvent {
  type: "TRIGGERED_SUBGRAPH_COMPLETED";
  /** Subworkflow ID */
  subgraphId: ID;
  /** Trigger ID */
  triggerId: ID;
  /** output data */
  output?: Record<string, unknown>;
  /** Execution time (milliseconds) */
  executionTime?: number;
}

/**
 * Trigger sub workflow failure event type
 */
export interface TriggeredSubgraphFailedEvent extends BaseEvent {
  type: "TRIGGERED_SUBGRAPH_FAILED";
  /** Subworkflow ID */
  subgraphId: ID;
  /** Trigger ID */
  triggerId: ID;
  /** error message */
  error: string;
  /** Execution time (milliseconds) */
  executionTime?: number;
}
