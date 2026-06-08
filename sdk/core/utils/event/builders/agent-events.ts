/**
 * Agent Event Builders
 * Provides builders for agent lifecycle events
 */

import { now, generateId } from "@wf-agent/common-utils";
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
  AgentHookTriggeredEvent,
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
export const buildAgentTurnStartedEvent =
  createBuilder<AgentTurnStartedEvent>("AGENT_TURN_STARTED");

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
export const buildAgentToolExecutionStartedEvent = createBuilder<AgentToolExecutionStartedEvent>(
  "AGENT_TOOL_EXECUTION_STARTED",
);

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
export const buildAgentIterationCompletedEvent = createBuilder<AgentIterationCompletedEvent>(
  "AGENT_ITERATION_COMPLETED",
);

// =============================================================================
// Agent Hook Events
// =============================================================================

/**
 * Build AGENT_HOOK_TRIGGERED event
 *
 * Manually constructed to support parentContext which is not representable
 * via createBuilder (nested sub-object with optional fields).
 *
 * This is the single canonical builder for hook triggered events used by both
 * EventRegistry (persistence) and streaming consumers.
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
  type: "AGENT_HOOK_TRIGGERED",
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
