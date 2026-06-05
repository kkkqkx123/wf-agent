/**
 * Custom Event Builders
 * Provides builders for custom events (NodeCustomEvent, AgentHookTriggeredEvent)
 */

import { now, generateId } from "@wf-agent/common-utils";
import { createBuilder } from "./common.js";
import type {
  NodeCustomEvent,
  AgentHookTriggeredEvent,
  AgentHookType,
  Metadata,
} from "@wf-agent/types";

// =============================================================================
// Node Custom Event Builder
// =============================================================================

/**
 * Build node custom event
 */
export const buildNodeCustomEvent = createBuilder<NodeCustomEvent>("NODE_CUSTOM_EVENT");

// =============================================================================
// Agent Hook Triggered Event Builder
// =============================================================================

/**
 * Build agent hook triggered event
 *
 * @remarks Manually constructed because "hook_triggered" is not part of EventType union.
 */
export const buildAgentHookTriggeredEvent = (params: {
  agentLoopId: string;
  agentLoopEntityId: string;
  hookType: AgentHookType;
  eventName: string;
  eventData: Record<string, unknown>;
  iteration: number;
  parentContext?: {
    parentType: "WORKFLOW" | "AGENT_LOOP";
    parentId: string;
    nodeId?: string;
    delegationPurpose?: string;
  };
  metadata?: Metadata;
}): AgentHookTriggeredEvent => ({
  id: generateId(),
  type: "hook_triggered",
  timestamp: now(),
  agentLoopId: params.agentLoopId,
  agentLoopEntityId: params.agentLoopEntityId,
  hookType: params.hookType,
  eventName: params.eventName,
  eventData: params.eventData,
  iteration: params.iteration,
  parentContext: params.parentContext,
  metadata: params.metadata,
});
