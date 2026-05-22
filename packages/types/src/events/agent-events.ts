/**
 * Agent Related Event Type Definitions
 *
 * These events represent the agent lifecycle and are integrated into the core Event system.
 * They enable querying, filtering, and persistence of agent execution history.
 *
 * Note: AgentHookTriggeredEvent is defined in agent-execution/event.ts for better separation.
 */

import type { ID, Metadata } from "../common.js";
import type { BaseEvent } from "./base.js";
import type { AgentHookType } from "../agent-execution/hooks.js";

/**
 * Agent Started Event
 * Emitted when an agent loop begins execution
 */
export interface AgentStartedEvent extends BaseEvent {
  type: "AGENT_STARTED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Maximum iterations configured */
  maxIterations: number;
  /** Initial message count */
  initialMessageCount: number;
}

/**
 * Agent Completed Event
 * Emitted when an agent loop finishes execution (success or failure)
 */
export interface AgentCompletedEvent extends BaseEvent {
  type: "AGENT_COMPLETED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Total iterations executed */
  iterations: number;
  /** Total tool calls made */
  toolCallCount: number;
  /** Success flag */
  success: boolean;
  /** Error if failed */
  error?: unknown;
}

/**
 * Agent Turn Started Event
 * Emitted at the beginning of each iteration (LLM call + tool execution cycle)
 */
export interface AgentTurnStartedEvent extends BaseEvent {
  type: "AGENT_TURN_STARTED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Current iteration number */
  iteration: number;
}

/**
 * Agent Turn Completed Event
 * Emitted when an iteration completes (after LLM response and tool executions)
 */
export interface AgentTurnCompletedEvent extends BaseEvent {
  type: "AGENT_TURN_COMPLETED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Iteration number */
  iteration: number;
  /** Whether another iteration will follow */
  shouldContinue: boolean;
  /** Reason for stopping (if not continuing) */
  stopReason?: string;
}

/**
 * Agent Message Started Event
 * Emitted when the agent begins receiving a message from LLM
 */
export interface AgentMessageStartedEvent extends BaseEvent {
  type: "AGENT_MESSAGE_STARTED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Iteration number */
  iteration: number;
}

/**
 * Agent Message Completed Event
 * Emitted when the agent finishes receiving a complete message from LLM
 */
export interface AgentMessageCompletedEvent extends BaseEvent {
  type: "AGENT_MESSAGE_COMPLETED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Iteration number */
  iteration: number;
  /** Message role */
  role: "assistant" | "user" | "system" | "tool";
}

/**
 * Agent Tool Execution Started Event
 * Emitted when the agent begins executing a tool
 */
export interface AgentToolExecutionStartedEvent extends BaseEvent {
  type: "AGENT_TOOL_EXECUTION_STARTED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Tool call ID */
  toolCallId: ID;
  /** Tool name */
  toolName: string;
  /** Iteration number */
  iteration: number;
}

/**
 * Agent Tool Execution Completed Event
 * Emitted when a tool execution finishes
 */
export interface AgentToolExecutionCompletedEvent extends BaseEvent {
  type: "AGENT_TOOL_EXECUTION_COMPLETED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Tool call ID */
  toolCallId: ID;
  /** Tool name */
  toolName: string;
  /** Execution success flag */
  success: boolean;
  /** Execution duration in milliseconds */
  duration?: number;
  /** Error if failed */
  error?: unknown;
  /** Iteration number */
  iteration: number;
}

/**
 * Agent Iteration Started Event
 * Emitted when a new iteration begins
 */
export interface AgentIterationStartedEvent extends BaseEvent {
  type: "AGENT_ITERATION_STARTED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Iteration number */
  iteration: number;
}

/**
 * Agent Iteration Completed Event
 * Emitted when all tool executions in an iteration complete
 */
export interface AgentIterationCompletedEvent extends BaseEvent {
  type: "AGENT_ITERATION_COMPLETED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Iteration number */
  iteration: number;
  /** Number of tool calls in this iteration */
  toolCallCount: number;
  /** Whether to continue to next iteration */
  shouldContinue: boolean;
}

/**
 * Agent Hook Triggered Event (Core Event System)
 *
 * Emitted when an agent hook is triggered during execution.
 * This is the core event system version that extends BaseEvent.
 *
 * Note: For streaming events, use AgentHookTriggeredEvent from agent-execution/event.ts
 */
export interface AgentHookTriggeredCoreEvent extends BaseEvent {
  type: "AGENT_HOOK_TRIGGERED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Agent loop entity ID for registry lookup */
  agentLoopEntityId: ID;
  /** Hook type that triggered this event */
  hookType: AgentHookType;
  /** Event name (from hook configuration) */
  eventName: string;
  /** Event payload data */
  eventData: Record<string, unknown>;
  /** Current iteration number */
  iteration: number;
  /** Additional metadata */
  metadata?: Metadata;
}

/**
 * Agent Paused Event
 *
 * Emitted when agent execution is paused by user request.
 */
export interface AgentPausedEvent extends BaseEvent {
  type: "AGENT_PAUSED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Current iteration when paused */
  iteration: number;
  /** Total tool calls made */
  toolCallCount: number;
  /** Whether was streaming when paused */
  isStreaming: boolean;
  /** Number of pending tool calls */
  pendingToolCalls: number;
  /** Whether streaming message was preserved */
  streamMessagePreserved: boolean;
  /** Reason for pause (optional) */
  reason?: string;
}

/**
 * Agent Cancelled Event
 *
 * Emitted when agent execution is cancelled/stopped by user request.
 */
export interface AgentCancelledEvent extends BaseEvent {
  type: "AGENT_CANCELLED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Current iteration when cancelled */
  iteration: number;
  /** Total tool calls made */
  toolCallCount: number;
  /** Whether was streaming when cancelled */
  isStreaming: boolean;
  /** Number of pending tool calls */
  pendingToolCalls: number;
  /** Reason for cancellation (optional) */
  reason?: string;
}

/**
 * Agent Resumed Event
 *
 * Emitted when agent execution is resumed from paused state.
 */
export interface AgentResumedEvent extends BaseEvent {
  type: "AGENT_RESUMED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Current iteration when resumed */
  iteration: number;
  /** Total tool calls made */
  toolCallCount: number;
}

/**
 * Agent Failed Event
 *
 * Emitted when agent execution fails due to an error.
 */
export interface AgentFailedEvent extends BaseEvent {
  type: "AGENT_FAILED";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Current iteration when failed */
  iteration: number;
  /** Total tool calls made */
  toolCallCount: number;
  /** Error that caused failure */
  error: unknown;
}

/**
 * Union type of all agent events
 */
export type AgentEvent =
  | AgentStartedEvent
  | AgentCompletedEvent
  | AgentTurnStartedEvent
  | AgentTurnCompletedEvent
  | AgentMessageStartedEvent
  | AgentMessageCompletedEvent
  | AgentToolExecutionStartedEvent
  | AgentToolExecutionCompletedEvent
  | AgentIterationStartedEvent
  | AgentIterationCompletedEvent
  | AgentHookTriggeredCoreEvent
  | AgentPausedEvent
  | AgentCancelledEvent
  | AgentResumedEvent
  | AgentFailedEvent;
