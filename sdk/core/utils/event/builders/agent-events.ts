/**
 * Agent Event Builders
 * Provides builders for agent lifecycle events
 */

import { createBuilder } from "./common.js";
import type {
  AgentStartedEvent,
  AgentCompletedEvent,
  AgentPausedEvent,
  AgentCancelledEvent,
  AgentResumedEvent,
  AgentFailedEvent,
  AgentTurnStartedEvent,
  AgentTurnCompletedEvent,
  AgentMessageStartedEvent,
  AgentMessageCompletedEvent,
  AgentToolExecutionStartedEvent,
  AgentToolExecutionCompletedEvent,
  AgentIterationStartedEvent,
  AgentIterationCompletedEvent,
  AgentHookTriggeredCoreEvent,
  AgentHookType,
  Metadata,
} from "@wf-agent/types";

// =============================================================================
// Agent Lifecycle Events
// =============================================================================

/**
 * Build AGENT_STARTED event
 */
export const buildAgentStartedEvent = createBuilder<AgentStartedEvent>("AGENT_STARTED");

/**
 * Build AGENT_COMPLETED event
 */
export const buildAgentCompletedEvent = createBuilder<AgentCompletedEvent>("AGENT_COMPLETED");

/**
 * Build AGENT_PAUSED event
 */
export const buildAgentPausedEvent = createBuilder<AgentPausedEvent>("AGENT_PAUSED");

/**
 * Build AGENT_CANCELLED event
 */
export const buildAgentCancelledEvent = createBuilder<AgentCancelledEvent>("AGENT_CANCELLED");

/**
 * Build AGENT_RESUMED event
 */
export const buildAgentResumedEvent = createBuilder<AgentResumedEvent>("AGENT_RESUMED");

/**
 * Build AGENT_FAILED event
 */
export const buildAgentFailedEvent = createBuilder<AgentFailedEvent>("AGENT_FAILED");

// =============================================================================
// Agent Turn Events
// =============================================================================

/**
 * Build AGENT_TURN_STARTED event
 */
export const buildAgentTurnStartedEvent = createBuilder<AgentTurnStartedEvent>("AGENT_TURN_STARTED");

/**
 * Build AGENT_TURN_COMPLETED event
 */
export const buildAgentTurnCompletedEvent =
  createBuilder<AgentTurnCompletedEvent>("AGENT_TURN_COMPLETED");

// =============================================================================
// Agent Message Events
// =============================================================================

/**
 * Build AGENT_MESSAGE_STARTED event
 */
export const buildAgentMessageStartedEvent =
  createBuilder<AgentMessageStartedEvent>("AGENT_MESSAGE_STARTED");

/**
 * Build AGENT_MESSAGE_COMPLETED event
 */
export const buildAgentMessageCompletedEvent =
  createBuilder<AgentMessageCompletedEvent>("AGENT_MESSAGE_COMPLETED");

// =============================================================================
// Agent Tool Execution Events
// =============================================================================

/**
 * Build AGENT_TOOL_EXECUTION_STARTED event
 */
export const buildAgentToolExecutionStartedEvent =
  createBuilder<AgentToolExecutionStartedEvent>("AGENT_TOOL_EXECUTION_STARTED");

/**
 * Build AGENT_TOOL_EXECUTION_COMPLETED event
 */
export const buildAgentToolExecutionCompletedEvent =
  createBuilder<AgentToolExecutionCompletedEvent>("AGENT_TOOL_EXECUTION_COMPLETED");

// =============================================================================
// Agent Iteration Events
// =============================================================================

/**
 * Build AGENT_ITERATION_STARTED event
 */
export const buildAgentIterationStartedEvent =
  createBuilder<AgentIterationStartedEvent>("AGENT_ITERATION_STARTED");

/**
 * Build AGENT_ITERATION_COMPLETED event
 */
export const buildAgentIterationCompletedEvent =
  createBuilder<AgentIterationCompletedEvent>("AGENT_ITERATION_COMPLETED");

// =============================================================================
// Agent Hook Events
// =============================================================================

/**
 * Build AGENT_HOOK_TRIGGERED core event (for EventRegistry)
 * Used to convert streaming events to core events
 */
export const buildAgentHookTriggeredCoreEvent = (
  params: {
    id: string;
    timestamp: number;
    agentLoopId: string;
    hookType: AgentHookType;
    eventName: string;
    eventData: Record<string, unknown>;
    iteration: number;
    parentWorkflowExecutionId?: string;
    nodeId?: string;
    metadata?: Metadata;
  },
): AgentHookTriggeredCoreEvent => ({
  id: params.id,
  type: "AGENT_HOOK_TRIGGERED",
  timestamp: params.timestamp,
  agentLoopId: params.agentLoopId,
  hookType: params.hookType,
  eventName: params.eventName,
  eventData: params.eventData,
  iteration: params.iteration,
  parentWorkflowExecutionId: params.parentWorkflowExecutionId,
  nodeId: params.nodeId,
  metadata: params.metadata,
});

// =============================================================================
// Agent Tool Management Events (NEW)
// =============================================================================

/**
 * Build agent tools added event
 */
export function buildAgentToolsAddedEvent(params: {
  agentLoopId: string;
  toolIds: string[];
  addedCount: number;
  iteration?: number;
}): AgentHookTriggeredCoreEvent {
  return {
    id: crypto.randomUUID(),
    type: "AGENT_HOOK_TRIGGERED",
    timestamp: Date.now(),
    agentLoopId: params.agentLoopId,
    hookType: "system" as AgentHookType,
    eventName: "tools_added",
    eventData: {
      toolIds: params.toolIds,
      addedCount: params.addedCount,
      iteration: params.iteration,
    },
    iteration: params.iteration ?? 0,
    metadata: {
      component: "AgentLoopEntity",
    },
  };
}

/**
 * Build agent tools removed event
 */
export function buildAgentToolsRemovedEvent(params: {
  agentLoopId: string;
  toolIds: string[];
  removedCount: number;
  iteration?: number;
}): AgentHookTriggeredCoreEvent {
  return {
    id: crypto.randomUUID(),
    type: "AGENT_HOOK_TRIGGERED",
    timestamp: Date.now(),
    agentLoopId: params.agentLoopId,
    hookType: "system" as AgentHookType,
    eventName: "tools_removed",
    eventData: {
      toolIds: params.toolIds,
      removedCount: params.removedCount,
      iteration: params.iteration,
    },
    iteration: params.iteration ?? 0,
    metadata: {
      component: "AgentLoopEntity",
    },
  };
}
