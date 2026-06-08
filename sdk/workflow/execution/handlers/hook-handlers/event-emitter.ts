/**
 * Hook Event Emitter
 *
 * Responsible for emitting Hook related custom events.
 */

import type { HookExecutionContext } from "./hook-handler.js";
import type { NodeCustomEvent } from "@wf-agent/types";
import { buildNodeCustomEvent } from "../../../../core/utils/event/builders/custom-events.js";
import { emitHookEventSafe } from "../../../../core/utils/event/emit-hook-event.js";

/**
 * Emit Hook custom event
 *
 * @param context Hook execution context
 * @param eventName Event name
 * @param eventData Event data
 * @param emitEvent Event emitting function
 */
export async function emitHookEvent(
  context: HookExecutionContext,
  eventName: string,
  eventData: Record<string, unknown>,
  emitEvent: (event: NodeCustomEvent) => Promise<void>,
): Promise<void> {
  const { workflowExecutionEntity, node } = context;
  const workflowExecution = workflowExecutionEntity.getExecution();

  const event = buildNodeCustomEvent({
    workflowId: workflowExecution.workflowId,
    executionId: workflowExecution.id,
    nodeId: node.id,
    nodeType: node.type,
    eventName,
    eventData,
    metadata: node.metadata,
  });

  await emitHookEventSafe(
    event,
    emitEvent,
    `Failed to emit custom event "${eventName}" for node "${node.id}"`,
    { operation: "emit", eventType: "NODE_CUSTOM_EVENT", nodeId: node.id },
    { eventName, nodeId: node.id },
  );
}