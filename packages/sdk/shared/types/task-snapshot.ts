/**
 * Task Snapshot Types and Utilities
 *
 * Defines Task snapshot structure and serialization utilities.
 *
 * Design notes:
 * - WorkflowExecutionResult is a pure data structure (all fields JSON-serializable),
 *   so it is used directly in TaskSnapshot without a separate serialized type.
 * - Only Error requires explicit serialization (via ErrorCodec) since Error
 *   objects are not directly JSON-serializable.
 */

import type {
  SnapshotBase,
  SerializedError,
  TaskStatus,
  WorkflowExecutionResult,
} from "@wf-agent/types";
import { ErrorCodec } from "@wf-agent/common-utils";

/**
 * Task Snapshot - Serializable representation of task data
 * WorkflowExecutionResult is stored directly as it is a pure data structure.
 */
export interface TaskSnapshot extends SnapshotBase {
  _entityType: "task";
  /** Task ID */
  id: string;
  /** Execution instance type (agent or workflowExecution) */
  instanceType: "agent" | "workflowExecution";
  /** Instance ID (executionId or agentLoopId) */
  instanceId: string;
  /** Workflow ID */
  workflowId: string;
  /** Execution ID (for workflow execution instances) */
  executionId?: string;
  /** Task Status */
  status: TaskStatus;
  /** Submission Time */
  submitTime: number;
  /** Start execution time */
  startTime?: number;
  /** Completion time */
  completeTime?: number;
  /** Execution result (upon success) - WorkflowExecutionResult is a pure data structure */
  result?: WorkflowExecutionResult;
  /** Error message (in case of failure) */
  error?: SerializedError;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}

/**
 * Utility functions for Task serialization
 *
 * Note: WorkflowExecutionResult is used directly (it is a pure data structure),
 * so no separate serialize/deserialize functions are needed for it.
 * Only Error objects require explicit serialization via ErrorCodec.
 */
export const TaskSerializationUtils = {
  /**
   * Create a TaskSnapshot from TaskInfo
   */
  createTaskSnapshotFromTaskInfo(taskInfo: {
    id: string;
    instanceType: "agent" | "workflowExecution";
    instance: { id: string; getExecutionId?: () => string; getWorkflowId?: () => string };
    status: TaskStatus;
    submitTime: number;
    startTime?: number;
    completeTime?: number;
    result?: WorkflowExecutionResult;
    error?: Error;
    timeout?: number;
  }): TaskSnapshot {
    const snapshot: TaskSnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: "task",
      id: taskInfo.id,
      instanceType: taskInfo.instanceType,
      instanceId: taskInfo.instance.id,
      workflowId: "",
      status: taskInfo.status,
      submitTime: taskInfo.submitTime,
      startTime: taskInfo.startTime,
      completeTime: taskInfo.completeTime,
      timeout: taskInfo.timeout,
    };

    if (taskInfo.instanceType === "workflowExecution") {
      snapshot.id = taskInfo.instance.id;
      if (taskInfo.instance.getWorkflowId) {
        snapshot.workflowId = taskInfo.instance.getWorkflowId();
      }
    } else {
      snapshot.workflowId = taskInfo.instance.id;
    }

    // WorkflowExecutionResult is a pure data structure, assign directly
    if (taskInfo.result) {
      snapshot.result = taskInfo.result;
    }

    if (taskInfo.error) {
      snapshot.error = ErrorCodec.serialize(taskInfo.error);
    }

    return snapshot;
  },
};
