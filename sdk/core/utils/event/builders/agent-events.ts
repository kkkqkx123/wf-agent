/**
 * Agent Event Builders
 * Provides builders for agent lifecycle events
 */

import { createBuilder, createErrorBuilder } from "./common.js";
import type {
  AgentStartedEvent,
  AgentCompletedEvent,
  AgentTurnStartedEvent,
  AgentTurnCompletedEvent,
  AgentMessageStartedEvent,
  AgentMessageCompletedEvent,
  AgentToolExecutionStartedEvent,
  AgentToolExecutionCompletedEvent,
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
