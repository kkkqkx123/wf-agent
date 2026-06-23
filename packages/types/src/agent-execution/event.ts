/**
 * Agent Event Type Definitions
 *
 * This module contains runtime event types for Agent Loop execution.
 * Part of the agent-execution package for runtime-related types.
 *
 * Note: Streaming events at the LLM layer (such as text increments, thought content, etc.)
 * are provided by Stream Event. Only events specific to the Agent layer (tool calls,
 * iteration completions, etc.) are defined here.
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

import type { ID, Metadata } from "../common.js";
import type { LLMMessage } from "../message/index.js";
import type { ToolExecutionResult } from "../tool/execution.js";
import type { AgentHookType } from "./hooks.js";

/**
 * Agent event types
 *
 * Contains only agent-specific events, not LLM level events
 *
 * Enhanced with fine-grained events for better observability
 */
export type AgentStreamEventType =
  // ========== Agent Lifecycle ==========
  | "agent_start"
  | "agent_end"
  // ========== Turn Lifecycle (one LLM call + tool executions) ==========
  | "turn_start"
  | "turn_end"
  // ========== Message Lifecycle ==========
  | "message_start"
  | "message_update"
  | "message_end"
  // ========== Tool Execution Lifecycle ==========
  | "tool_execution_start"
  | "tool_execution_update"
  | "tool_execution_end"
  // ========== Iteration ==========
  | "iteration_start"
  | "iteration_complete"
  // ========== Error ==========
  | "agent_error"
  // ========== Steering & Follow-up ==========
  | "steering_injected"
  | "followup_queued"
  // ========== Hook Events ==========
  | "hook_triggered"
  // ========== Interruption Events ==========
  | "agent_paused"
  | "agent_cancelled";

/**
 * Agent Start Event
 */
export interface AgentStartEvent {
  type: "agent_start";
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
  type: "agent_end";
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
  type: "turn_start";
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
  type: "turn_end";
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
  type: "message_start";
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
  type: "message_update";
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
  type: "message_end";
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
  type: "tool_execution_start";
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
  type: "tool_execution_update";
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
  type: "tool_execution_end";
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
 * Iteration Start Event
 */
export interface IterationStartEvent {
  type: "iteration_start";
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Iteration number */
  iteration: number;
}

/**
 * Iteration Complete Event
 */
export interface IterationCompleteEvent {
  type: "iteration_complete";
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
  type: "agent_error";
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
  type: "steering_injected";
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
  type: "followup_queued";
  timestamp: number;
  /** Agent loop ID */
  agentLoopId: string;
  /** Follow-up message */
  message: LLMMessage;
}

/**
 * Agent Hook Triggered Stream Event
 *
 * Streaming variant of AgentHookTriggeredEvent for the AgentStreamEvent system.
 * Uses lowercase "hook_triggered" type to maintain the AgentStreamEvent naming convention.
 *
 * For the canonical (EventRegistry) version, see AgentHookTriggeredEvent in events/agent-events.ts.
 */
export interface AgentHookTriggeredStreamEvent {
  /** Unique event identifier */
  id: ID;
  type: "hook_triggered";
  timestamp: number;
  /** Agent loop ID */
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
  /** Parent execution context (unified hierarchy) */
  parentContext?: {
    parentType: 'WORKFLOW' | 'AGENT_LOOP';
    parentId: string;
    nodeId?: string;
    delegationPurpose?: string;
  };
  /** Additional metadata */
  metadata?: Metadata;
}

/**
 * Agent Paused Event
 *
 * Emitted when agent execution is paused by user request.
 */
export interface AgentPausedEvent {
  type: "agent_paused";
  timestamp: number;
  /** Agent loop ID */
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
export interface AgentCancelledEvent {
  type: "agent_cancelled";
  timestamp: number;
  /** Agent loop ID */
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
  | IterationStartEvent
  | IterationCompleteEvent
  | AgentErrorEvent
  | SteeringInjectedEvent
  | FollowupQueuedEvent
  | AgentHookTriggeredStreamEvent
  | AgentPausedEvent
  | AgentCancelledEvent;
