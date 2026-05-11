/**
 * Task Snapshot Types and Utilities
 *
 * Defines Task snapshot structure and serialization utilities.
 */

import type {
  SnapshotBase,
  SerializedError,
  TaskStatus,
  WorkflowExecutionResult,
  NodeExecutionResult,
  WorkflowExecutionStatus,
} from "@wf-agent/types";
import { ErrorCodec } from "@wf-agent/common-utils";

/**
 * Task Snapshot - Serializable representation of task data
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
  /** Execution result (upon success) - serialized with full data */
  result?: SerializedWorkflowExecutionResult;
  /** Error message (in case of failure) */
  error?: SerializedError;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}

/**
 * Serialized WorkflowExecution Result Metadata
 */
export interface SerializedWorkflowExecutionResultMetadata {
  /** Workflow execution state status */
  status: WorkflowExecutionStatus;
  /** Starting time */
  startTime: number;
  /** End time */
  endTime: number;
  /** Execution time (milliseconds) */
  executionTime: number;
  /** Number of nodes */
  nodeCount: number;
  /** Number of errors */
  errorCount: number;
}

/**
 * Serialized WorkflowExecution Result
 */
export interface SerializedWorkflowExecutionResult {
  /** Execution ID */
  id: string;
  /** Output data */
  output: Record<string, unknown>;
  /** Execution time (milliseconds) */
  executionTime: number;
  /** Complete array of node execution results */
  nodeResults: NodeExecutionResult[];
  /** Complete metadata */
  metadata: SerializedWorkflowExecutionResultMetadata;
}

/**
 * Utility functions for Task serialization
 */
export const TaskSerializationUtils = {
  /**
   * Serialize WorkflowExecutionResult for storage
   */
  serializeWorkflowExecutionResult(result: WorkflowExecutionResult): SerializedWorkflowExecutionResult {
    return {
      id: result.executionId,
      output: result.output,
      executionTime: result.executionTime,
      nodeResults: result.nodeResults,
      metadata: {
        status: result.metadata.status,
        startTime: result.metadata.startTime,
        endTime: result.metadata.endTime,
        executionTime: result.metadata.executionTime,
        nodeCount: result.metadata.nodeCount,
        errorCount: result.metadata.errorCount,
      },
    };
  },

  /**
   * Deserialize WorkflowExecutionResult from serialized format
   */
  deserializeWorkflowExecutionResult(serialized: SerializedWorkflowExecutionResult): WorkflowExecutionResult {
    return {
      executionId: serialized.id,
      output: serialized.output,
      executionTime: serialized.executionTime,
      nodeResults: serialized.nodeResults,
      metadata: {
        status: serialized.metadata.status,
        startTime: serialized.metadata.startTime,
        endTime: serialized.metadata.endTime,
        executionTime: serialized.metadata.executionTime,
        nodeCount: serialized.metadata.nodeCount,
        errorCount: serialized.metadata.errorCount,
      },
    };
  },

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

    if (taskInfo.result) {
      snapshot.result = TaskSerializationUtils.serializeWorkflowExecutionResult(taskInfo.result);
    }

    if (taskInfo.error) {
      snapshot.error = ErrorCodec.serialize(taskInfo.error);
    }

    return snapshot;
  },
};
