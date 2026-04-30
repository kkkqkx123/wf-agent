/**
 * Custom Event Builders
 * Provides builders for custom events (NodeCustomEvent, AgentCustomEvent)
 */

import { now } from "@wf-agent/common-utils";
import type { NodeCustomEvent, AgentCustomEvent, Metadata } from "@wf-agent/types";

// =============================================================================
// Node Custom Event Builder
// =============================================================================

/**
 * Build node custom event
 */
export const buildNodeCustomEvent = (params: {
  workflowId: string;
  executionId: string;
  nodeId: string;
  nodeType: string;
  eventName: string;
  eventData: Record<string, unknown>;
  metadata?: Metadata;
}): NodeCustomEvent => ({
  type: "NODE_CUSTOM_EVENT",
  timestamp: now(),
  ...params,
});

// =============================================================================
// Agent Custom Event Builder
// =============================================================================

/**
 * Build agent custom event
 */
export const buildAgentCustomEvent = (params: {
  executionId: string;
  agentLoopId: string;
  eventName: string;
  eventData: Record<string, unknown>;
  iteration?: number;
  parentExecutionId?: string;
  nodeId?: string;
  metadata?: Metadata;
}): AgentCustomEvent => ({
  type: "AGENT_CUSTOM_EVENT",
  timestamp: now(),
  ...params,
});
