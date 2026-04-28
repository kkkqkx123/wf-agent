/**
 * Workflow Execution Type Definitions
 * Defines the types associated with diagram workflow execution
 *
 * These types describe the configuration and result of the operation "Execute a workflow".
 * WorkflowExecution is the top-level execution module of a graph workflow and contains complete information about the graph structure.
 */

import type { WorkflowExecution, WorkflowExecutionOptions, WorkflowExecutionResult } from "./index.js";
import type { WorkflowTemplate } from "../workflow/index.js";
import type { ID, Timestamp, Metadata } from "../common.js";

/**
 * Harmonized implementation status enumeration (as a type reference only)
 *
 * This enumeration is used to provide a uniform naming reference for execution states.
 * AgentLoopEntity and WorkflowExecutionEntity may refer to this enumeration to define status, but continue to use their respective AgentLoopStatus and WorkflowExecutionStatus.
 * But continue to use their respective AgentLoopStatus and WorkflowExecutionStatus.
 *
 * Mapping relationships (for reference only):
 * - AgentLoopStatus.CREATED / WorkflowExecutionStatus.CREATED -> ExecutionStatus.PENDING
 * - AgentLoopStatus.RUNNING / WorkflowExecutionStatus.RUNNING -> ExecutionStatus.RUNNING
 * - AgentLoopStatus.PAUSED / WorkflowExecutionStatus.PAUSED -> ExecutionStatus.PAUSED
 * - AgentLoopStatus.COMPLETED / WorkflowExecutionStatus.COMPLETED -> ExecutionStatus.COMPLETED
 * - AgentLoopStatus.FAILED / WorkflowExecutionStatus.FAILED -> ExecutionStatus.FAILED
 * - AgentLoopStatus.CANCELLED / WorkflowExecutionStatus.CANCELLED -> ExecutionStatus.CANCELLED
 * - WorkflowExecutionStatus.TIMEOUT -> ExecutionStatus.FAILED
 */
export enum ExecutionStatus {
  /** Pending execution */
  PENDING = "PENDING",
  /** In execution */
  RUNNING = "RUNNING",
  /** Paused */
  PAUSED = "PAUSED",
  /** Completed */
  COMPLETED = "COMPLETED",
  /** Execution failed */
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
  /** Instance paused */
  INSTANCE_PAUSED = "INSTANCE_PAUSED",
  /** Instance resumed */
  INSTANCE_RESUMED = "INSTANCE_RESUMED",
  /** Instance completed */
  INSTANCE_COMPLETED = "INSTANCE_COMPLETED",
  /** Instance execution failed */
  INSTANCE_FAILED = "INSTANCE_FAILED",
  /** Instance cancelled */
  INSTANCE_CANCELLED = "INSTANCE_CANCELLED",

  // Execution process events
  /** Node started executing */
  NODE_STARTED = "NODE_STARTED",
  /** Node execution completed */
  NODE_COMPLETED = "NODE_COMPLETED",
  /** Node execution failed */
  NODE_FAILED = "NODE_FAILED",

  // Tool invocation events
  /** Tool call started */
  TOOL_CALL_STARTED = "TOOL_CALL_STARTED",
  /** Tool call completed */
  TOOL_CALL_COMPLETED = "TOOL_CALL_COMPLETED",
  /** Tool call failed */
  TOOL_CALL_FAILED = "TOOL_CALL_FAILED",

  // Checkpoint events
  /** Checkpoint created */
  CHECKPOINT_CREATED = "CHECKPOINT_CREATED",
  /** Checkpoint restored */
  CHECKPOINT_RESTORED = "CHECKPOINT_RESTORED",
}
