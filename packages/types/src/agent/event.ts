/**
 * Agent event type definition
 *
 * Note: Streaming events at the LLM layer (such as text increments, thought content, etc.) are provided by Stream Event,
 * Only events specific to the Agent layer (tool calls, iteration completions, etc.) are defined here.
 *
 * Architecture design:
 * - LLM API Streaming Response → MessageStream → MessageStreamEvent
 * - AgentLoopExecutor subscribes to MessageStreamEvent and generates AgentStreamEvent
 * - Upper consumers receive two types of events at the same time
 *
 * Enhanced Event System (inspired by pi-agent-core):
 * - Fine-grained events for turn/message/tool lifecycle
 * - Supports steering and follow-up mechanisms
 */

import type { LLMMessage } from "../message/index.js";
import type { ToolCall } from "../tool/execution.js";
import type { ToolExecutionResult } from "../tool/execution.js";

/**
 * Agent event types
 *
 * Contains only agent-specific events, not LLM level events
 *
 * Enhanced with fine-grained events for better observability
 */
export enum AgentStreamEventType {
  // ========== Agent Lifecycle ==========
  /** Agent started processing */
  AGENT_START = "agent_start",
  /** Agent completed processing */
  AGENT_END = "agent_end",

  // ========== Turn Lifecycle (one LLM call + tool executions) ==========
  /** Turn started (new iteration) */
  TURN_START = "turn_start",
  /** Turn completed */
  TURN_END = "turn_end",

  // ========== Message Lifecycle ==========
  /** Message started (user/assistant/toolResult) */
  MESSAGE_START = "message_start",
  /** Message updated (streaming delta) */
  MESSAGE_UPDATE = "message_update",
  /** Message completed */
  MESSAGE_END = "message_end",

  // ========== Tool Execution Lifecycle ==========
  /** Tool execution started */
  TOOL_EXECUTION_START = "tool_execution_start",
  /** Tool execution progress update */
  TOOL_EXECUTION_UPDATE = "tool_execution_update",
  /** Tool execution completed */
  TOOL_EXECUTION_END = "tool_execution_end",

  // ========== Iteration ==========
  /** Iteration completed */
  ITERATION_COMPLETE = "iteration_complete",

  // ========== Error ==========
  /** Error occurred */
  ERROR = "agent_error",

  // ========== Steering & Follow-up ==========
  /** Steering message injected */
  STEERING_INJECTED = "steering_injected",
  /** Follow-up message queued */
  FOLLOWUP_QUEUED = "followup_queued",
}

/**
 * Agent Start Event
 */
export interface AgentStartEvent {
  type: AgentStreamEventType.AGENT_START;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Max iterations */
  maxIterations: number;
  /** Initial message count */
  initialMessageCount: number;
}

/**
 * Agent End Event
 */
export interface AgentEndEvent {
  type: AgentStreamEventType.AGENT_END;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** All messages generated */
  messages: LLMMessage[];
  /** Total iterations */
  iterations: number;
  /** Total tool calls */
  toolCallCount: number;
  /** Success flag */
  success: boolean;
  /** Error if failed */
  error?: unknown;
}

/**
 * Turn Start Event
 */
export interface TurnStartEvent {
  type: AgentStreamEventType.TURN_START;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Current iteration number */
  iteration: number;
}

/**
 * Turn End Event
 */
export interface TurnEndEvent {
  type: AgentStreamEventType.TURN_END;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Iteration number */
  iteration: number;
  /** Assistant message */
  message: LLMMessage;
  /** Tool results */
  toolResults: ToolExecutionResult[];
}

/**
 * Message Start Event
 */
export interface MessageStartEvent {
  type: AgentStreamEventType.MESSAGE_START;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Message */
  message: LLMMessage;
}

/**
 * Message Update Event (streaming delta)
 */
export interface MessageUpdateEvent {
  type: AgentStreamEventType.MESSAGE_UPDATE;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Partial message delta */
  delta: Partial<LLMMessage>;
}

/**
 * Message End Event
 */
export interface MessageEndEvent {
  type: AgentStreamEventType.MESSAGE_END;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Complete message */
  message: LLMMessage;
}

/**
 * Tool Execution Start Event
 */
export interface ToolExecutionStartEvent {
  type: AgentStreamEventType.TOOL_EXECUTION_START;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** Tool arguments */
  args: unknown;
  /** Iteration number */
  iteration: number;
}

/**
 * Tool Execution Update Event (streaming progress)
 */
export interface ToolExecutionUpdateEvent {
  type: AgentStreamEventType.TOOL_EXECUTION_UPDATE;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Tool call ID */
  toolCallId: string;
  /** Partial result */
  partialResult: unknown;
}

/**
 * Tool Execution End Event
 */
export interface ToolExecutionEndEvent {
  type: AgentStreamEventType.TOOL_EXECUTION_END;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** Execution result */
  result: ToolExecutionResult;
  /** Execution duration (ms) */
  duration: number;
}

/**
 * Iteration Complete Event
 */
export interface IterationCompleteEvent {
  type: AgentStreamEventType.ITERATION_COMPLETE;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Iteration number */
  iteration: number;
  /** Whether to continue */
  shouldContinue: boolean;
  /** Reason for stopping (if not continuing) */
  stopReason?: string;
}

/**
 * Error Event
 */
export interface AgentErrorEvent {
  type: AgentStreamEventType.ERROR;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Error */
  error: unknown;
  /** Iteration when error occurred */
  iteration: number;
  /** Context */
  context?: string;
}

/**
 * Steering Injected Event
 */
export interface SteeringInjectedEvent {
  type: AgentStreamEventType.STEERING_INJECTED;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Steering message */
  message: LLMMessage;
  /** Skipped tool calls */
  skippedToolCalls: string[];
}

/**
 * Follow-up Queued Event
 */
export interface FollowupQueuedEvent {
  type: AgentStreamEventType.FOLLOWUP_QUEUED;
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Follow-up message */
  message: LLMMessage;
}

/**
 * Agent Stream Event (union type)
 *
 * All possible agent stream events
 */
export type AgentStreamEvent =
  | AgentStartEvent
  | AgentEndEvent
  | TurnStartEvent
  | TurnEndEvent
  | MessageStartEvent
  | MessageUpdateEvent
  | MessageEndEvent
  | ToolExecutionStartEvent
  | ToolExecutionUpdateEvent
  | ToolExecutionEndEvent
  | IterationCompleteEvent
  | AgentErrorEvent
  | SteeringInjectedEvent
  | FollowupQueuedEvent;
