/**
 * Agent Hook Event Emitter
 *
 * Responsible for emitting Agent Hook triggered events.
 * Uses the new AgentHookTriggeredEvent type instead of deprecated AgentCustomEvent.
 */

import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { AgentHookTriggeredEvent, AgentHookType } from "@wf-agent/types";
import { EventSystemError } from "@wf-agent/types";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { buildAgentHookTriggeredEvent } from "../../../../core/utils/event/builders/index.js";

/**
 * Agent Hook Event Data
 *
 * Data for building AgentHookTriggeredEvent.
 */
export interface AgentHookEventData {
  /** Agent Loop ID */
  agentLoopId: string;
  /** Hook type that triggered the event */
  hookType: AgentHookType;
  /** Event name */
  eventName: string;
  /** Event data */
  eventData: Record<string, unknown>;
  /** Current iteration count */
  iteration: number;
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: string;
  /** Node ID (if executed as a Graph node) */
  nodeId?: string;
}

/**
 * Emit Agent Hook Triggered Event
 *
 * @param entity Agent Loop entity
 * @param hookType Type of hook that triggered
 * @param eventName Event name
 * @param eventData Event data
 * @param emitEvent Event emitting function
 */
export async function emitAgentHookEvent(
  entity: AgentLoopEntity,
  hookType: AgentHookType,
  eventName: string,
  eventData: Record<string, unknown> | undefined,
  emitEvent: (event: AgentHookTriggeredEvent) => Promise<void>,
): Promise<void> {
  const parentContext = entity.getParentContext();
  const event = buildAgentHookTriggeredEvent({
    agentLoopId: entity.id,
    hookType,
    eventName,
    eventData: eventData ?? {},
    iteration: entity.state.currentIteration,
    // Keep old fields for backward compatibility
    parentWorkflowExecutionId: parentContext?.parentType === 'WORKFLOW' ? parentContext.parentId : undefined,
    nodeId: parentContext?.parentType === 'WORKFLOW' ? parentContext.nodeId : undefined,
    // Add new unified parent context
    parentContext: parentContext ? {
      parentType: parentContext.parentType,
      parentId: parentContext.parentId,
      ...(parentContext.parentType === 'WORKFLOW' && { nodeId: parentContext.nodeId }),
      ...(parentContext.parentType === 'AGENT_LOOP' && { delegationPurpose: parentContext.delegationPurpose }),
    } : undefined,
    metadata: {
      profileId: entity.config.profileId,
      toolCallCount: entity.state.toolCallCount,
    },
  });

  try {
    await emitEvent(event);
  } catch (error) {
    throw new EventSystemError(
      `Failed to emit agent hook event "${eventName}" for agent loop "${entity.id}"`,
      "emit",
      "HOOK_TRIGGERED",
      entity.nodeId,
      undefined,
      { eventName, hookType, agentLoopId: entity.id, originalError: getErrorOrNew(error) },
    );
  }
}
