/**
 * Basic Event Type Definition
 */

import type { ID, Timestamp, Metadata } from "../common.js";

/**
 * Event Type
 */
export type EventType =
  /** Workflow Execution Start */
  | "WORKFLOW_EXECUTION_STARTED"
  /** Workflow Execution completion */
  | "WORKFLOW_EXECUTION_COMPLETED"
  /** Workflow Execution Failure */
  | "WORKFLOW_EXECUTION_FAILED"
  /** Workflow Execution Pause */
  | "WORKFLOW_EXECUTION_PAUSED"
  /** Workflow Execution Recovery */
  | "WORKFLOW_EXECUTION_RESUMED"
  /** Workflow Execution Cancellation */
  | "WORKFLOW_EXECUTION_CANCELLED"
  /** Workflow Execution state change */
  | "WORKFLOW_EXECUTION_STATE_CHANGED"
  /** Workflow Execution Forking Begins */
  | "WORKFLOW_EXECUTION_FORK_STARTED"
  /** Workflow Execution Forking Complete */
  | "WORKFLOW_EXECUTION_FORK_COMPLETED"
  /** Workflow Execution merge started */
  | "WORKFLOW_EXECUTION_JOIN_STARTED"
  /** Workflow Execution merge condition satisfied */
  | "WORKFLOW_EXECUTION_JOIN_CONDITION_MET"
  /** Workflow Execution merge completed */
  | "WORKFLOW_EXECUTION_JOIN_COMPLETED"
  /** Workflow Execution merge failed */
  | "WORKFLOW_EXECUTION_JOIN_FAILED"
  /** Workflow Execution copying begins */
  | "WORKFLOW_EXECUTION_COPY_STARTED"
  /** Workflow Execution copying complete */
  | "WORKFLOW_EXECUTION_COPY_COMPLETED"
  /** nodal start */
  | "NODE_STARTED"
  /** Node Completion */
  | "NODE_COMPLETED"
  /** Node Failure */
  | "NODE_FAILED"
  /** Node customization events */
  | "NODE_CUSTOM_EVENT"
  /** Fork node started */
  | "FORK_STARTED"
  /** Fork branch started */
  | "FORK_BRANCH_STARTED"
  /** Fork branch completed */
  | "FORK_BRANCH_COMPLETED"
  /** Fork node completed */
  | "FORK_COMPLETED"
  /** Token exceeds limit */
  | "TOKEN_LIMIT_EXCEEDED"
  /** Token Usage Warning */
  | "TOKEN_USAGE_WARNING"
  /** Context Compression Request */
  | "CONTEXT_COMPRESSION_REQUESTED"
  /** Context compression complete */
  | "CONTEXT_COMPRESSION_COMPLETED"
  /** Message Addition */
  | "MESSAGE_ADDED"
  /** Start of tool call */
  | "TOOL_CALL_STARTED"
  /** Tool call completion */
  | "TOOL_CALL_COMPLETED"
  /** Tool call failure */
  | "TOOL_CALL_FAILED"
  /** Tool call blocked (failure protection) */
  | "TOOL_CALL_BLOCKED"
  /** Tool Additions */
  | "TOOL_ADDED"
  /** Tool visibility changed */
  | "TOOL_VISIBILITY_CHANGED"
  /** Dialog status change */
  | "CONVERSATION_STATE_CHANGED"
  /** error event */
  | "ERROR"
  /** Checkpoint Creation */
  | "CHECKPOINT_CREATED"
  /** Checkpoint recovery */
  | "CHECKPOINT_RESTORED"
  /** Checkpoint deletion */
  | "CHECKPOINT_DELETED"
  /** Checkpoint Failure */
  | "CHECKPOINT_FAILED"
  /** Start of subgraphs */
  | "SUBGRAPH_STARTED"
  /** Submap Completion */
  | "SUBGRAPH_COMPLETED"
  /** Trigger the start of a sub-workflow */
  | "TRIGGERED_SUBGRAPH_STARTED"
  /** Trigger sub workflow completion */
  | "TRIGGERED_SUBGRAPH_COMPLETED"
  /** Failure to trigger a sub workflow */
  | "TRIGGERED_SUBGRAPH_FAILED"
  /** Variable change */
  | "VARIABLE_CHANGED"
  /** Tool Approval Requested (specific) */
  | "TOOL_APPROVAL_REQUESTED"
  /** Tool Approval Responded */
  | "TOOL_APPROVAL_RESPONDED"
  /** Tool Approval Failed */
  | "TOOL_APPROVAL_FAILED"
  /** Follow-up Question Requested (specific) */
  | "FOLLOWUP_QUESTION_REQUESTED"
  /** Follow-up Question Responded */
  | "FOLLOWUP_QUESTION_RESPONDED"
  /** Follow-up Question Failed */
  | "FOLLOWUP_QUESTION_FAILED"
  /** LLM Flow Abort */
  | "LLM_STREAM_ABORTED"
  /** LLM Streaming Error */
  | "LLM_STREAM_ERROR"
  /** Skill loading begins */
  | "SKILL_LOAD_STARTED"
  /** Skill loading complete */
  | "SKILL_LOAD_COMPLETED"
  /** Skill load failed */
  | "SKILL_LOAD_FAILED"
  /** Async completion registered */
  | "ASYNC_COMPLETION_REGISTERED"
  /** Async completion triggered successfully */
  | "ASYNC_COMPLETION_TRIGGERED"
  /** Async completion error triggered */
  | "ASYNC_COMPLETION_ERROR_TRIGGERED"
  /** Async completion execution failed */
  | "ASYNC_COMPLETION_FAILED"
  /** Async completion cleaned up */
  | "ASYNC_COMPLETION_CLEANED_UP"
  /** Agent started */
  | "AGENT_STARTED"
  /** Agent completed */
  | "AGENT_COMPLETED"
  /** Agent turn started */
  | "AGENT_TURN_STARTED"
  /** Agent turn completed */
  | "AGENT_TURN_COMPLETED"
  /** Agent message started */
  | "AGENT_MESSAGE_STARTED"
  /** Agent message completed */
  | "AGENT_MESSAGE_COMPLETED"
  /** Agent tool execution started */
  | "AGENT_TOOL_EXECUTION_STARTED"
  /** Agent tool execution completed */
  | "AGENT_TOOL_EXECUTION_COMPLETED"
  /** Timeout registered */
  | "TIMEOUT_REGISTERED"
  /** Timeout expired */
  | "TIMEOUT_EXPIRED"
  /** Timeout cancelled */
  | "TIMEOUT_CANCELLED"
  /** Timeout warning threshold reached */
  | "TIMEOUT_WARNING"
  /** Agent iteration started */
  | "AGENT_ITERATION_STARTED"
  /** Agent iteration completed */
  | "AGENT_ITERATION_COMPLETED"
  /** Agent hook triggered */
  | "AGENT_HOOK_TRIGGERED"
  /** Agent paused */
  | "AGENT_PAUSED"
  /** Agent cancelled */
  | "AGENT_CANCELLED"
  /** Agent resumed */
  | "AGENT_RESUMED"
  /** Agent failed */
  | "AGENT_FAILED"
  /** Abstract execution pause (domain-agnostic, emitted by InterruptionState) */
  | "EXECUTION_PAUSED"
  /** Abstract execution cancel (domain-agnostic, emitted by InterruptionState) */
  | "EXECUTION_CANCELLED"
  /** Abstract execution resume (domain-agnostic, emitted by InterruptionState) */
  | "EXECUTION_RESUMED"
  /** Progressive tool execution start (batch) */
  | "PROGRESSIVE_TOOL_EXECUTION_START"
  /** Progressive tool execution end (batch) */
  | "PROGRESSIVE_TOOL_EXECUTION_END"
  /** Tool queue update (progressive) */
  | "TOOL_QUEUE_UPDATE"
  /** Tool approval annotated */
  | "TOOL_APPROVAL_ANNOTATED"
  /** SYNC node started */
  | "NODE_SYNC_STARTED"
  /** SYNC node completed */
  | "NODE_SYNC_COMPLETED"
  /** SYNC node failed */
  | "NODE_SYNC_FAILED";

/**
 * Basic Event Types
 */
export interface BaseEvent {
  /** Unique event identifier */
  id: ID;
  /** Event Type */
  type: EventType;
  /** timestamp */
  timestamp: Timestamp;
  /** Workflow ID (optional) */
  workflowId?: ID;
  /** Execution ID (optional, for events that do not depend on the graph layer, such as the core layer) */
  executionId?: ID;
  /** Agent Loop ID (optional, for agent-related events) */
  agentLoopId?: ID;
  /** event metadata */
  metadata?: Metadata;
}

/**
 * Event Listener Types
 */
export type EventListener<T extends BaseEvent> = (event: T) => void | Promise<void>;

/**
 * Event Handler Type
 */
export interface EventHandler {
  /** Event Type */
  eventType: EventType;
  /** event listener */
  listener: EventListener<BaseEvent>;
}

/**
 * Listener Options Interface
 * Provides configuration options for event listener registration
 */
export interface ListenerOptions<T extends BaseEvent = BaseEvent> {
  /** Execution order priority (higher values execute first) */
  priority?: number;
  /** Selective event handling filter function */
  filter?: (event: T) => boolean;
  /** Listener execution timeout in milliseconds */
  timeout?: number;
  /** Associate listener with specific execution for auto-cleanup */
  executionId?: string;
  /** Enable automatic cleanup when execution ends (default: true) */
  autoCleanup?: boolean;
}
