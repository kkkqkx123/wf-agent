/**
 * Custom Event Builders
 * Provides builders for custom events (NodeCustomEvent, AgentHookTriggeredEvent)
 */

import { now, generateId } from "@wf-agent/common-utils";
import type { NodeCustomEvent, AgentHookTriggeredEvent, AgentHookType, Metadata, AgentStreamEventType } from "@wf-agent/types";

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
  id: generateId(),
  type: "NODE_CUSTOM_EVENT",
  timestamp: now(),
  ...params,
});

// =============================================================================
// Agent Hook Triggered Event Builder
// =============================================================================

/**
 * Build agent hook triggered event
 *
 * This replaces the deprecated AgentCustomEvent with a more structured approach.
 */
export const buildAgentHookTriggeredEvent = (params: {
  agentLoopId: string;
  hookType: AgentHookType;
  eventName: string;
  eventData: Record<string, unknown>;
  iteration: number;
  parentWorkflowExecutionId?: string;
  nodeId?: string;
  parentContext?: {
    parentType: 'WORKFLOW' | 'AGENT_LOOP';
    parentId: string;
    nodeId?: string;
    delegationPurpose?: string;
  };
  metadata?: Metadata;
}): AgentHookTriggeredEvent => ({
  id: generateId(),
  type: "hook_triggered" as AgentStreamEventType.HOOK_TRIGGERED,
  timestamp: now(),
  agentLoopId: params.agentLoopId,
  hookType: params.hookType,
  eventName: params.eventName,
  eventData: params.eventData,
  iteration: params.iteration,
  // Keep old fields for backward compatibility
  parentWorkflowExecutionId: params.parentWorkflowExecutionId,
  nodeId: params.nodeId,
  // Add new unified parent context
  parentContext: params.parentContext,
  metadata: params.metadata,
});
