/**
 * Workflow Execution Type Definitions
 * Defines the types associated with diagram workflow execution
 *
 * These types describe the configuration and result of the operation "Execute a workflow".
 * Thread is the top-level execution module of a graph workflow and contains complete information about the graph structure.
 */

import type { WorkflowExecution, ThreadOptions, ThreadResult } from "./index.js";
import type { WorkflowTemplate } from "../workflow/index.js";
import type { ID, Timestamp, Metadata } from "../common.js";

/**
 * Type of workflow execution option
 */
export interface WorkflowExecutionOptions {
  /** Workflow definitions */
  workflow: WorkflowTemplate;
  /** Threading Options */
  threadOptions?: ThreadOptions;
  /** Whether to enable event listening */
  enableEvents?: boolean;
  /** Whether to enable logging */
  enableLogging?: boolean;
  /** Customizing the execution context */
  context?: Metadata;
}

/**
 * Type of workflow execution result
 */
export interface WorkflowExecutionResult {
  /** Successful implementation */
  success: boolean;
  /** Thread results */
  threadResult: ThreadResult;
  /** Implementation metadata */
  metadata: WorkflowExecutionMetadata;

  // Backward compatibility properties (delegate to threadResult)
  /** @deprecated Use threadResult.threadId instead */
  get threadId(): ID;
  /** @deprecated Use threadResult.output instead */
  get output(): Record<string, unknown>;
  /** @deprecated Use threadResult.executionTime instead */
  get executionTime(): Timestamp;
  /** @deprecated Use threadResult.nodeResults instead */
  get nodeResults(): import("./history.js").NodeExecutionResult[];
}

/**
 * Workflow execution metadata types
 */
export interface WorkflowExecutionMetadata {
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: ID;
  /** Thread ID */
  threadId: ID;
  /** Starting time */
  startTime: Timestamp;
  /** end time */
  endTime: Timestamp;
  /** Execution time (milliseconds) */
  duration: number;
  /** Number of execution steps */
  steps: number;
  /** Number of nodes executed */
  nodesExecuted: number;
  /** Number of edges executed */
  edgesTraversed: number;
  /** Whether checkpoints are used */
  usedCheckpoints: boolean;
  /** Number of checkpoints */
  checkpointCount: number;
  /** Custom Fields */
  customFields?: Metadata;
}

/**
 * Workflow execution context type
 */
export interface WorkflowExecutionContext {
  /** Workflow definitions */
  workflow: WorkflowTemplate;
  /** current thread */
  thread: WorkflowExecution;
  /** implementation option */
  options: WorkflowExecutionOptions;
  /** Implementation metadata */
  metadata: WorkflowExecutionMetadata;
  /** Customized Contextual Data */
  contextData: Metadata;
}

/**
 * Harmonized implementation status enumeration (as a type reference only)
 * 
 * This enumeration is used to provide a uniform naming reference for execution states.
 * AgentLoopEntity and ThreadEntity may refer to this enumeration to define status, but continue to use their respective AgentLoopStatus and ThreadStatus.
 * But continue to use their respective AgentLoopStatus and ThreadStatus.
 * 
 * Mapping relationships (for reference only):
 * - AgentLoopStatus.CREATED / ThreadStatus.CREATED -> ExecutionStatus.PENDING
 * - AgentLoopStatus.RUNNING / ThreadStatus.RUNNING -> ExecutionStatus.RUNNING
 * - AgentLoopStatus.PAUSED / ThreadStatus.PAUSED -> ExecutionStatus.PAUSED
 * - AgentLoopStatus.COMPLETED / ThreadStatus.COMPLETED -> ExecutionStatus.COMPLETED
 * - AgentLoopStatus.FAILED / ThreadStatus.FAILED -> ExecutionStatus.FAILED
 * - AgentLoopStatus.CANCELLED / ThreadStatus.CANCELLED -> ExecutionStatus.CANCELLED
 * - ThreadStatus.TIMEOUT -> ExecutionStatus.FAILED
 */
export enum ExecutionStatus {
  /** awaiting implementation */
  PENDING = "PENDING",
  /** under implementation */
  RUNNING = "RUNNING",
  /** Suspended */
  PAUSED = "PAUSED",
  /** done */
  COMPLETED = "COMPLETED",
  /** failure of execution */
  FAILED = "FAILED",
  /** Cancelled */
  CANCELLED = "CANCELLED",
}

/**
 * Harmonized enumeration of execution event types (as a type reference only)
 *
 * This enumeration is used to provide a uniform naming reference for execution events.
 * Graph already has an EventRegistry, no additional implementation is needed.
 * Agent Loop does not require a separate event system.
 * This enumeration is used as a reference for event naming only.
 */
export enum ExecutionEventType {
  // Instance lifecycle events
  /** Instance created */
  INSTANCE_CREATED = "INSTANCE_CREATED",
  /** Instance started */
  INSTANCE_STARTED = "INSTANCE_STARTED",
  /** Instance has been suspended */
  INSTANCE_PAUSED = "INSTANCE_PAUSED",
  /** Instance restored */
  INSTANCE_RESUMED = "INSTANCE_RESUMED",
  /** Example completed */
  INSTANCE_COMPLETED = "INSTANCE_COMPLETED",
  /** Instance execution failure */
  INSTANCE_FAILED = "INSTANCE_FAILED",
  /** Instance canceled */
  INSTANCE_CANCELLED = "INSTANCE_CANCELLED",

  // implementation process event (computing)
  /** The node starts executing */
  NODE_STARTED = "NODE_STARTED",
  /** Node execution completed */
  NODE_COMPLETED = "NODE_COMPLETED",
  /** Node execution failure */
  NODE_FAILED = "NODE_FAILED",

  // Tool invocation events
  /** Start of tool call */
  TOOL_CALL_STARTED = "TOOL_CALL_STARTED",
  /** Tool call completion */
  TOOL_CALL_COMPLETED = "TOOL_CALL_COMPLETED",
  /** Tool call failure */
  TOOL_CALL_FAILED = "TOOL_CALL_FAILED",

  // checkpoint event
  /** Checkpoint created */
  CHECKPOINT_CREATED = "CHECKPOINT_CREATED",
  /** Checkpoints have been restored */
  CHECKPOINT_RESTORED = "CHECKPOINT_RESTORED",
}
