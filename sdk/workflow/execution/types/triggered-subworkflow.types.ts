/**
 * Trigger workflow-related type definitions
 *
 * Design principles:
 * - Simple and clear type definitions
 * - Support for both synchronous and asynchronous execution modes
 * - Provide a complete lifecycle status for trigger workflows
 */

import type { ID } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import { TaskStatus } from "../../../core/types/index.js";

/**
 * Trigger Sub-workflow Task Interface
 */
export interface TriggeredSubgraphTask {
  /** Sub-workflow ID */
  subgraphId: ID;
  /** Input data */
  input: Record<string, unknown>;
  /** Trigger ID */
  triggerId: string;
  /** Main Workflow Execution Entity */
  mainWorkflowExecutionEntity: WorkflowExecutionEntity;
  /** Configuration options */
  config?: {
    /**
     * Whether to wait for the sub-workflow to complete:
     * - true: Execute synchronously (default); the caller will be blocked until the sub-workflow is completed.
     * - false: Execute asynchronously; the caller will return immediately, and the sub-workflow will run in the background.
     */
    waitForCompletion?: boolean;
    /** Timeout period (in milliseconds) */
    timeout?: number;
    /** Should history be recorded? */
    recordHistory?: boolean;
    /** metadata */
    metadata?: Record<string, unknown>;
  };
}

/**
 * Execute the return result of a single trigger workflow (synchronous execution)
 */
export interface ExecutedSubgraphResult {
  /** Sub-workflow entity */
  subgraphEntity: WorkflowExecutionEntity;
  /** Execution result */
  executionResult: WorkflowExecutionResult;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * Task submission result (asynchronous execution)
 */
export interface TaskSubmissionResult {
  /** Task ID */
  taskId: string;
  /** Task Status */
  status: TaskStatus;
  /** Message */
  message: string;
  /** Submission time (in milliseconds) */
  submitTime: number;
}

/**
 * Queue Task Interface (for internal use)
 */
export interface QueueTask {
  /** Task ID */
  taskId: string;
  /** Workflow Execution Entity */
  workflowExecutionEntity: WorkflowExecutionEntity;
  /** Promise resolve function */
  resolve: (value: ExecutedSubgraphResult) => void;
  /** Promise reject function */
  reject: (error: Error) => void;
  /** Submission time */
  submitTime: number;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}
