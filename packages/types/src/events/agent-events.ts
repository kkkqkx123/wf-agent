/**
 * Agent Related Event Type Definitions
 */

import type { ID, Metadata } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Agent Custom Event Types
 *
 * Custom Events for Agent Hook Triggering
 */
export interface AgentCustomEvent extends BaseEvent {
  type: "AGENT_CUSTOM_EVENT";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Custom Event Names */
  eventName: string;
  /** Event data */
  eventData: Record<string, unknown>;
  /** Current number of iterations */
  iteration?: number;
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
  /** event metadata */
  metadata?: Metadata;
}
