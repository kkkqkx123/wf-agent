/**
 * Agent Related Event Type Definitions
 *
 * These events represent the agent lifecycle and are integrated into the core Event system.
 * They enable querying, filtering, and persistence of agent execution history.
 */

import type { ID, Metadata } from "../common.js";
import type { BaseEvent } from "./base.js";

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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
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
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
}

/**
 * Agent Custom Event Types
 *
 * Custom Events for Agent Hook Triggering
 * @deprecated Use specific agent event types above instead
 */
export interface AgentCustomEvent extends BaseEvent {
  type: "AGENT_CUSTOM_EVENT";
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Custom Event Names */
  eventName: string;
  /** Event data */
  eventData: Record<string, unknown>;
  /** Current number of iterations */
  iteration?: number;
  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;
  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;
  /** event metadata */
  metadata?: Metadata;
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
  | AgentIterationCompletedEvent
  | AgentCustomEvent;
