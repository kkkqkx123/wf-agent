/**
 * Workflow Tool Types
 *
 * Design Principles:
 * - Defines SDK-specific types for workflow builtin tools
 * - Provides type-safe interfaces for workflow operations
 * - Located in SDK to avoid circular dependencies with packages/types
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionRegistry } from "../../registry/workflow-execution-registry.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import type { WorkflowExecutionBuilder } from "../factories/workflow-execution-builder.js";
import type { WorkflowRegistry } from "../../registry/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../../registry/workflow-graph-registry.js";
import type { GlobalContext } from "../../../shared/global-context.js";
import { z } from "zod";

/**
 * Type guard to check if context is a valid WorkflowToolExecutionContext
 */
export function isWorkflowToolExecutionContext(
  context: unknown,
): context is WorkflowToolExecutionContext {
  const ctx = context as Partial<WorkflowToolExecutionContext>;
  return ctx.globalContext !== undefined && ctx.parentExecutionEntity !== undefined;
}

/**
 * Assert that context is a valid WorkflowToolExecutionContext, throws error if not
 */
export function assertWorkflowContext(
  context: unknown,
): asserts context is WorkflowToolExecutionContext {
  if (!isWorkflowToolExecutionContext(context)) {
    throw new Error(
      "Invalid workflow context: globalContext and parentExecutionEntity are required",
    );
  }
}

/**
 * Workflow Tool Execution Context
 * Provides type-safe context for workflow builtin tool execution
 *
 * This interface extends the base BuiltinToolExecutionContext with
 * SDK-specific types, ensuring type safety while avoiding circular dependencies.
 */
export interface WorkflowToolExecutionContext {
  /** Current execution ID */
  executionId?: string;
  /** Parent workflow execution entity */
  parentExecutionEntity?: WorkflowExecutionEntity;
  /** Workflow execution registry */
  executionRegistry?: WorkflowExecutionRegistry;
  /** Event manager */
  eventManager?: EventRegistry;
  /** Workflow execution builder */
  workflowExecutionBuilder?: WorkflowExecutionBuilder;
  /** Task queue manager (deprecated) */
  taskQueueManager?: any;
  /** Workflow registry */
  workflowRegistry?: WorkflowRegistry;
  /** Graph registry */
  graphRegistry?: WorkflowGraphRegistry;
  /** Global context for accessing DI container */
  globalContext?: GlobalContext;
}

/**
 * Workflow Task Status Enum
 * Aligned with TaskStatus from @wf-agent/types for consistency
 */
export type WorkflowTaskStatus =
  | "QUEUED"
  | "RUNNING"
  | "COMPLETED"
  | "FAILED"
  | "CANCELLED"
  | "TIMEOUT"
  | "NOT_FOUND"
  | "ERROR";

/**
 * Execute Workflow Parameters Schema (for runtime validation)
 */
export const ExecuteWorkflowParamsSchema = z.object({
  workflowId: z.string(),
  input: z.record(z.string(), z.unknown()).default({}),
  messageContexts: z
    .record(
      z.string(),
      z.array(
        z.object({
          role: z.enum(["user", "assistant", "system", "tool"]),
          content: z.union([z.string(), z.array(z.any())]),
        }),
      ),
    )
    .optional(),
  waitForCompletion: z.boolean().default(true),
  timeout: z.number().positive().max(300000).optional(), // Max 5 minutes
});

/**
 * Execute Workflow Parameters
 */
export type ExecuteWorkflowParams = z.infer<typeof ExecuteWorkflowParamsSchema>;

/**
 * Execute Workflow Success Result (Synchronous)
 */
export interface ExecuteWorkflowSuccessResult {
  /** Success flag */
  success: true;
  /** Execution status */
  status: "COMPLETED";
  /** Workflow output */
  output: Record<string, unknown>;
  /** Execution time in milliseconds */
  executionTime: number;
}

/**
 * Execute Workflow Error Result (Synchronous)
 */
export interface ExecuteWorkflowErrorResult {
  /** Success flag */
  success: false;
  /** Execution status */
  status: "FAILED" | "TIMEOUT" | "CANCELLED";
  /** Error details */
  error: {
    code: string;
    message: string;
    details?: unknown;
  };
  /** Execution time in milliseconds */
  executionTime?: number;
}

/**
 * Execute Workflow Result (Synchronous) - Union of success and error
 */
export type ExecuteWorkflowSyncResult = ExecuteWorkflowSuccessResult | ExecuteWorkflowErrorResult;

/**
 * Execute Workflow Result (Asynchronous)
 */
export interface ExecuteWorkflowAsyncResult {
  /** Success flag */
  success: true;
  /** Execution status */
  status: "SUBMITTED";
  /** Task ID for tracking */
  taskId: string;
  /** Status message */
  message: string;
}

/**
 * Execute Workflow Result
 */
export type ExecuteWorkflowResult = ExecuteWorkflowSyncResult | ExecuteWorkflowAsyncResult;

/**
 * Query Workflow Status Parameters Schema
 */
export const QueryWorkflowStatusParamsSchema = z.object({
  taskId: z.string(),
});

/**
 * Query Workflow Status Parameters
 */
export type QueryWorkflowStatusParams = z.infer<typeof QueryWorkflowStatusParamsSchema>;

/**
 * Query Workflow Status Result
 */
export interface QueryWorkflowStatusResult {
  /** Success flag */
  success: boolean;
  /** Task status */
  status?: WorkflowTaskStatus;
  /** Status message */
  message?: string;
  /** Execution ID */
  executionId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Creation timestamp */
  createdAt?: number;
  /** Update timestamp */
  updatedAt?: number;
}

/**
 * Cancel Workflow Parameters Schema
 */
export const CancelWorkflowParamsSchema = z.object({
  taskId: z.string(),
});

/**
 * Cancel Workflow Parameters
 */
export type CancelWorkflowParams = z.infer<typeof CancelWorkflowParamsSchema>;

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
