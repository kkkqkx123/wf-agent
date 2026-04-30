/**
 * Execution Types - Task and Execution Instance Definitions
 *
 * Unified type definitions for task management and execution instances.
 * Supports both Agent and Thread execution instances.
 *
 * Design Principles:
 * - Cross-module shared types within SDK core
 * - Support both synchronous and asynchronous execution modes
 * - Type safety with discriminated unions
 */

import type { WorkflowExecutionResult, TaskStatus } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../workflow/entities/workflow-execution-entity.js";
import type { AgentLoopEntity } from "../../agent/entities/agent-loop-entity.js";

// ============================================================================
// Execution Instance Types
// ============================================================================

/**
 * Execution Instance Type
 * Used to distinguish between Agent and WorkflowExecution execution instances
 */
export type ExecutionInstanceType = "agent" | "workflowExecution";

/**
 * Unified execution instance type
 * Supports both AgentLoopEntity and WorkflowExecutionEntity
 */
export type ExecutionInstance = AgentLoopEntity | WorkflowExecutionEntity;

// ============================================================================
// Type Guard Functions for Execution Instances
// ============================================================================

/**
 * Check if the execution instance is an AgentLoopEntity
 * @param instance Execution instance
 * @returns True if the instance is an AgentLoopEntity
 */
export function isAgentInstance(instance: ExecutionInstance): instance is AgentLoopEntity {
  return "config" in instance && "conversationManager" in instance;
}

/**
 * Check if the execution instance is a WorkflowExecutionEntity
 * @param instance Execution instance
 * @returns True if the instance is a WorkflowExecutionEntity
 */
export function isWorkflowExecutionInstance(instance: ExecutionInstance): instance is WorkflowExecutionEntity {
  return "getThreadId" in instance && "getWorkflowId" in instance;
}

/**
 * Get the execution instance type
 * @param instance Execution instance
 * @returns The type of the execution instance
 */
export function getExecutionInstanceType(instance: ExecutionInstance): ExecutionInstanceType {
  return isAgentInstance(instance) ? "agent" : "workflowExecution";
}

/**
 * Get the ID of the execution instance
 * @param instance Execution instance
 * @returns The ID of the execution instance
 */
export function getExecutionInstanceId(instance: ExecutionInstance): string {
  return instance.id;
}

// ============================================================================
// Task Types
// ============================================================================

// Re-export TaskStatus for convenience
export { TaskStatus } from "@wf-agent/types";

/**
 * Instance reference state
 * Used to track whether the instance is loaded or needs to be restored
 */
export type InstanceRef =
  | { type: "loaded"; instance: ExecutionInstance }
  | { type: "reference"; instanceId: string };

/**
 * Task Information Interface
 */
export interface TaskInfo {
  /** Task ID */
  id: string;
  /** Execution instance type (agent or thread) */
  instanceType: ExecutionInstanceType;
  /** Execution instance (AgentLoopEntity or WorkflowExecutionEntity) */
  instance: ExecutionInstance;
  /** Task Status */
  status: TaskStatus;
  /** Submission Time */
  submitTime: number;
  /** Start execution time */
  startTime?: number;
  /** Completion time */
  completeTime?: number;
  /** Execution result (upon success) */
  result?: WorkflowExecutionResult;
  /** Error message (in case of failure) */
  error?: Error;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}

/**
 * Stored Task Information Interface
 * Used when loading tasks from storage where the instance may not be immediately available
 */
export interface StoredTaskInfo {
  /** Task ID */
  id: string;
  /** Execution instance type (agent or thread) */
  instanceType: ExecutionInstanceType;
  /** Instance reference - may be a reference only if not yet loaded */
  instanceRef: InstanceRef;
  /** Task Status */
  status: TaskStatus;
  /** Submission Time */
  submitTime: number;
  /** Start execution time */
  startTime?: number;
  /** Completion time */
  completeTime?: number;
  /** Execution result (upon success) */
  result?: WorkflowExecutionResult;
  /** Error message (in case of failure) */
  error?: Error;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}

/**
 * Type guard to check if a task info has a loaded instance
 * @param taskInfo TaskInfo or StoredTaskInfo
 * @returns True if the instance is loaded
 */
export function hasLoadedInstance(taskInfo: TaskInfo | StoredTaskInfo): taskInfo is TaskInfo {
  return "instance" in taskInfo && taskInfo.instance !== undefined;
}

/**
 * Type guard to check if a task info is a StoredTaskInfo with instance reference
 * @param taskInfo TaskInfo or StoredTaskInfo
 * @returns True if the task uses instance reference
 */
export function isStoredTaskInfo(taskInfo: TaskInfo | StoredTaskInfo): taskInfo is StoredTaskInfo {
  return "instanceRef" in taskInfo;
}
