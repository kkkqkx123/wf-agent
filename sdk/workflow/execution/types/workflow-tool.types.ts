/**
 * Workflow Tool Types
 *
 * Design Principles:
 * - Defines SDK-specific types for workflow builtin tools
 * - Provides type-safe interfaces for workflow operations
 * - Located in SDK to avoid circular dependencies with packages/types
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { WorkflowExecutionBuilder } from "../factories/workflow-execution-builder.js";
import type { TaskQueue } from "../../stores/task/task-queue.js";
import type { WorkflowRegistry } from "../../stores/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../../stores/workflow-graph-registry.js";

/**
 * Workflow Tool Execution Context
 * Provides type-safe context for workflow builtin tool execution
 *
 * This interface extends the base BuiltinToolExecutionContext with
 * SDK-specific types, ensuring type safety while avoiding circular dependencies.
 */
export interface WorkflowToolExecutionContext {
  /** Current thread ID */
  threadId?: string;
  /** Parent thread entity */
  parentThreadEntity?: WorkflowExecutionEntity;
  /** Thread registry */
  threadRegistry?: WorkflowExecutionRegistry;
  /** Event manager */
  eventManager?: EventRegistry;
  /** Workflow execution builder */
  workflowExecutionBuilder?: WorkflowExecutionBuilder;
  /** Task queue manager */
  taskQueueManager?: TaskQueue;
  /** Workflow registry */
  workflowRegistry?: WorkflowRegistry;
  /** Graph registry */
  graphRegistry?: WorkflowGraphRegistry;
}

/**
 * Execute Workflow Parameters
 */
export interface ExecuteWorkflowParams {
  /** Workflow ID to execute */
  workflowId: string;
  /** Input parameters for the workflow */
  input?: Record<string, unknown>;
  /** Whether to wait for completion (default: true) */
  waitForCompletion?: boolean;
  /** Timeout in milliseconds */
  timeout?: number;
}

/**
 * Execute Workflow Result (Synchronous)
 */
export interface ExecuteWorkflowSyncResult {
  /** Success flag */
  success: boolean;
  /** Execution status */
  status: "completed";
  /** Workflow output */
  output?: Record<string, unknown>;
  /** Execution time in milliseconds */
  executionTime?: number;
}

/**
 * Execute Workflow Result (Asynchronous)
 */
export interface ExecuteWorkflowAsyncResult {
  /** Success flag */
  success: boolean;
  /** Execution status */
  status: "submitted";
  /** Task ID for tracking */
  taskId?: string;
  /** Status message */
  message?: string;
}

/**
 * Execute Workflow Result
 */
export type ExecuteWorkflowResult = ExecuteWorkflowSyncResult | ExecuteWorkflowAsyncResult;

/**
 * Query Workflow Status Parameters
 */
export interface QueryWorkflowStatusParams {
  /** Task ID to query */
  taskId: string;
}

/**
 * Query Workflow Status Result
 */
export interface QueryWorkflowStatusResult {
  /** Success flag */
  success: boolean;
  /** Task status */
  status?: string;
  /** Status message */
  message?: string;
  /** Thread ID */
  threadId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Creation timestamp */
  createdAt?: number;
  /** Update timestamp */
  updatedAt?: number;
}

/**
 * Cancel Workflow Parameters
 */
export interface CancelWorkflowParams {
  /** Task ID to cancel */
  taskId: string;
}

/**
 * Cancel Workflow Result
 */
export interface CancelWorkflowResult {
  /** Success flag */
  success: boolean;
  /** Task ID */
  taskId: string;
  /** Status message */
  message: string;
}
