/**
 * Agent Hook Event Emitter
 *
 * Responsible for emitting Agent Hook related custom events.
 * Referenced from Graph module's event-emitter.ts design.
 */

import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { AgentCustomEvent } from "@wf-agent/types";
import { EventSystemError } from "@wf-agent/types";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { buildAgentCustomEvent } from "../../../../core/utils/event/builders/index.js";

/**
 * Agent Custom Event Data
 *
 * Extends BaseEvent to add Agent-specific fields.
 */
export interface AgentCustomEventData {
  /** Agent Loop ID */
  agentLoopId: string;
  /** Event name */
  eventName: string;
  /** Event data */
  eventData: Record<string, unknown>;
  /** Current iteration count */
  iteration?: number;
  /** Parent Thread ID (if executed as a Graph node) */
  parentThreadId?: string;
  /** Node ID (if executed as a Graph node) */
  nodeId?: string;
}

/**
 * Emit Agent custom event
 *
 * @param entity Agent Loop entity
 * @param eventName Event name
 * @param eventData Event data
 * @param emitEvent Event emitting function
 */
export async function emitAgentHookEvent(
  entity: AgentLoopEntity,
  eventName: string,
  eventData: Record<string, unknown> | undefined,
  emitEvent: (event: AgentCustomEvent) => Promise<void>,
): Promise<void> {
  const event = buildAgentCustomEvent({
    threadId: entity.parentThreadId || entity.id, // Use parentThreadId or entity.id as threadId
    agentLoopId: entity.id,
    eventName,
    eventData: eventData ?? {},
    iteration: entity.state.currentIteration,
    parentThreadId: entity.parentThreadId,
    nodeId: entity.nodeId,
    metadata: {
      profileId: entity.config.profileId,
      toolCallCount: entity.state.toolCallCount,
    },
  });

  try {
    await emitEvent(event);
  } catch (error) {
    // Throw an event system error, which is handled centrally by higher layers
    throw new EventSystemError(
      `Failed to emit agent custom event "${eventName}" for agent loop "${entity.id}"`,
      "emit",
      "AGENT_CUSTOM_EVENT",
      entity.nodeId,
      undefined,
      { eventName, agentLoopId: entity.id, originalError: getErrorOrNew(error) },
    );
  }
}
