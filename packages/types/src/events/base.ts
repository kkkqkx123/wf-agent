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
  /** Tool Additions */
  | "TOOL_ADDED"
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
  /** User Interaction Requests */
  | "USER_INTERACTION_REQUESTED"
  /** User Interaction Response */
  | "USER_INTERACTION_RESPONDED"
  /** User interaction processing complete */
  | "USER_INTERACTION_PROCESSED"
  /** User interaction failure */
  | "USER_INTERACTION_FAILED"
  /** HumanRelay request */
  | "HUMAN_RELAY_REQUESTED"
  /** HumanRelay Response */
  | "HUMAN_RELAY_RESPONDED"
  /** HumanRelay processing complete. */
  | "HUMAN_RELAY_PROCESSED"
  /** HumanRelay Failed */
  | "HUMAN_RELAY_FAILED"
  /** LLM Flow Abort */
  | "LLM_STREAM_ABORTED"
  /** LLM Streaming Error */
  | "LLM_STREAM_ERROR"
  /** Agent Custom Events */
  | "AGENT_CUSTOM_EVENT"
  /** Skill loading begins */
  | "SKILL_LOAD_STARTED"
  /** Skill loading complete */
  | "SKILL_LOAD_COMPLETED"
  /** Skill load failed */
  | "SKILL_LOAD_FAILED";

/**
 * Basic Event Types
 */
export interface BaseEvent {
  /** Event Type */
  type: EventType;
  /** timestamp */
  timestamp: Timestamp;
  /** Workflow ID (optional) */
  workflowId?: ID;
  /** Execution ID (optional, for events that do not depend on the graph layer, such as the core layer) */
  executionId?: ID;
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
