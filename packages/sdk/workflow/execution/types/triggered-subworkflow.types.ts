/**
 * Trigger workflow-related type definitions
 *
 * Design principles:
 * - Simple and clear type definitions
 * - Support for both synchronous and asynchronous execution modes
 * - Provide a complete lifecycle status for trigger workflows
 */

import type { ID, ExecuteTriggeredSubworkflowActionConfig, LLMMessage } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/index.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";

/**
 * Data source type for triggered subworkflow input
 *
 * Determines which execution entity provides the base input data:
 * - 'workflow': Data comes from WorkflowExecutionEntity (default)
 * - 'agent': Data comes from AgentLoopEntity (agent hook triggered)
 */
export type DataSourceType = "workflow" | "agent";

/**
 * Resolved data source - unified view of source execution data
 *
 * Provides a consistent interface regardless of whether the source
 * is a workflow execution or an agent loop execution.
 */
export interface ResolvedDataSource {
  /** Source type */
  type: DataSourceType;
  /** Source entity ID */
  entityId: ID;
  /** Agent conversation messages (agent source only) */
  messages?: LLMMessage[];
  /** Agent execution state (agent source only) */
  agentState?: {
    currentIteration: number;
    toolCallCount: number;
    status: string;
  };
  /** Workflow output (workflow source only) */
  output?: Record<string, unknown>;
  /** Workflow input (workflow source only) */
  workflowInput?: Record<string, unknown>;
  /** Workflow execution variables */
  variables?: Record<string, unknown>;
}

/**
 * Trigger Sub-workflow Task Interface
 *
 * Represents a task to execute a triggered subworkflow.
 * All configuration including input/output mapping should be passed via the `config` field.
 */
export interface TriggeredSubworkflowTask {
  /** Sub-workflow ID (the workflow definition to execute) */
  subworkflowId: ID;
  /** Input data (variables) - will be merged with config.inputMapping if present */
  input: Record<string, unknown>;
  /** Trigger ID - unique identifier for this trigger event */
  triggerId: string;
  /** Main Workflow Execution Entity - the parent execution context */
  mainWorkflowExecutionEntity: WorkflowExecutionEntity;
  /** Configuration options including input/output mapping, timeout, etc. */
  config?: ExecuteTriggeredSubworkflowActionConfig;
  /**
   * Data source type - determines which execution entity provides base input
   *
   * - 'workflow': Use WorkflowExecutionEntity (default, backward compatible)
   * - 'agent': Use AgentLoopEntity (when triggered by agent hook event)
   */
  sourceType: DataSourceType;
  /**
   * Source entity ID - used for lazy lookup of source execution data
   * When sourceType is 'agent', this should be the AgentLoopEntity ID
   */
  sourceEntityId?: ID;
}

/**
 * Execute the return result of a single trigger subworkflow (synchronous execution)
 *
 * This interface provides comprehensive access to the subworkflow execution results.
 * Use `executionResult.output` for the final output data, or access `subworkflowEntity`
 * for internal state inspection and debugging purposes.
 */
export interface ExecutedSubworkflowResult {
  /**
   * Subworkflow execution entity - provides access to internal state, variables, and metadata.
   * Use this for debugging or when you need to inspect the execution context.
   * For normal usage, prefer `executionResult.output` to get the final output.
   */
  subworkflowEntity: WorkflowExecutionEntity;
  /**
   * Execution result containing output data, status, and other metadata.
   * This is the primary source for retrieving the subworkflow's output.
   */
  executionResult: WorkflowExecutionResult;
  /** Execution time in milliseconds */
  executionTime: number;
}

/**
 * Task submission result (asynchronous execution)
 */
export interface TaskSubmissionResult {
  /** Task ID */
  taskId: string;
  /** Task Status - QUEUED: successfully added to queue, REJECTED: rejected due to capacity or validation */
  status: "QUEUED" | "REJECTED";
  /** Message */
  message: string;
  /** Submission time (in milliseconds) */
  submitTime: number;
}

/**
 * Queue Task Interface for Triggered Subworkflow (for internal use)
 *
 * Note: This is specific to triggered subworkflow execution.
 * For generic execution queue tasks, see sdk/shared/execution/execution-queue.ts
 */
export interface TriggeredSubworkflowQueueTask {
  /** Task ID */
  taskId: string;
  /** Workflow Execution Entity */
  workflowExecutionEntity: WorkflowExecutionEntity;
  /** Promise resolve function */
  resolve: (value: ExecutedSubworkflowResult) => void;
  /** Promise reject function */
  reject: (error: Error) => void;
  /** Submission time */
  submitTime: number;
  /** Timeout period (in milliseconds) */
  timeout?: number;
}
