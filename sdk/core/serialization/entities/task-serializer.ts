/**
 * Task Snapshot Serializer
 *
 * Handles serialization and deserialization of Task snapshots.
 */

import { Serializer, ErrorSerializer } from "../serializer.js";
import { DeltaCalculator } from "../delta-calculator.js";
import { SerializationRegistry } from "../serialization-registry.js";
import type {
  SnapshotBase,
  SerializedError,
  TaskStatus,
  ThreadResult,
  NodeExecutionResult,
  ThreadStatus,
} from "@wf-agent/types";

/**
 * Task Snapshot - Serializable representation of task data
 */
export interface TaskSnapshot extends SnapshotBase {
  _entityType: "task";
  /** Task ID */
  id: string;
  /** Execution instance type (agent or thread) */
  instanceType: "agent" | "thread";
  /** Instance ID (threadId or agentLoopId) */
  instanceId: string;
  /** Workflow ID */
  workflowId: string;
  /** Thread ID (for thread instances) */
  threadId?: string;
  /** Task Status */
  status: TaskStatus;
  /** Submission Time */
  submitTime: number;
  /** Start execution time */
  startTime?: number;
  /** Completion time */
  completeTime?: number;
  /** Execution result (upon success) - serialized with full data */
  result?: SerializedThreadResult;
  /** Error message (in case of failure) */
  error?: SerializedError;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}

/**
 * Serialized Thread Result Metadata
 */
export interface SerializedThreadResultMetadata {
  /** Thread state status */
  status: ThreadStatus;
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
 * Serialized Thread Result
 */
export interface SerializedThreadResult {
  /** Thread ID */
  threadId: string;
  /** Output data */
  output: Record<string, unknown>;
  /** Execution time (milliseconds) */
  executionTime: number;
  /** Complete array of node execution results */
  nodeResults: NodeExecutionResult[];
  /** Complete metadata */
  metadata: SerializedThreadResultMetadata;
}

/**
 * Task Snapshot Serializer
 */
export class TaskSnapshotSerializer extends Serializer<TaskSnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }
}

/**
 * Task Delta Calculator
 */
export class TaskDeltaCalculator extends DeltaCalculator<TaskSnapshot> {
  constructor() {
    super({
      deepCompare: true,
      ignoreFields: ["_timestamp", "submitTime"],
    });
  }
}

/**
 * Utility functions for Task serialization
 */
export const TaskSerializationUtils = {
  /**
   * Serialize ThreadResult for storage
   */
  serializeThreadResult(result: ThreadResult): SerializedThreadResult {
    return {
      threadId: result.threadId,
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
   * Deserialize ThreadResult from serialized format
   */
  deserializeThreadResult(serialized: SerializedThreadResult): ThreadResult {
    return {
      threadId: serialized.threadId,
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
    instanceType: "agent" | "thread";
    instance: { id: string; getThreadId?: () => string; getWorkflowId?: () => string };
    status: TaskStatus;
    submitTime: number;
    startTime?: number;
    completeTime?: number;
    result?: ThreadResult;
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

    if (taskInfo.instanceType === "thread") {
      snapshot.threadId = taskInfo.instance.id;
      if (taskInfo.instance.getWorkflowId) {
        snapshot.workflowId = taskInfo.instance.getWorkflowId();
      }
    } else {
      snapshot.workflowId = taskInfo.instance.id;
    }

    if (taskInfo.result) {
      snapshot.result = TaskSerializationUtils.serializeThreadResult(taskInfo.result);
    }

    if (taskInfo.error) {
      snapshot.error = ErrorSerializer.serialize(taskInfo.error);
    }

    return snapshot;
  },
};

/**
 * Register Task serializer with the global registry
 */
export function registerTaskSerializer(): void {
  const registry = SerializationRegistry.getInstance();

  registry.register({
    entityType: "task",
    serializer: new TaskSnapshotSerializer(),
    deltaCalculator: new TaskDeltaCalculator(),
  });
}
