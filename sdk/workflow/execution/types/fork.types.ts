/**
 * Fork Execution Types
 * 
 * Type definitions for FORK node execution including handler context
 * and execution configuration.
 */

import type { WorkflowExecutionBuilder } from "../factories/workflow-execution-builder.js";
import type { WorkflowExecutor } from "../executors/workflow-executor.js";

/**
 * Fork Node Handler Context
 * 
 * Provides dependencies needed for FORK node execution.
 * Created by NodeHandlerContextFactory for FORK nodes.
 */
export interface ForkHandlerContext {
  /** Execution builder for creating fork branch entities */
  executionBuilder: WorkflowExecutionBuilder;
  /** Workflow executor for executing branches */
  workflowExecutor: WorkflowExecutor;
}

/**
 * Fork Execution Configuration
 * 
 * Runtime configuration for fork execution behavior.
 */
export interface ForkExecutionConfig {
  /** Fork strategy: parallel (default) or serial */
  strategy: 'parallel' | 'serial';
  /** Maximum concurrent branches (for resource control) */
  maxConcurrency?: number;
  /** Timeout for all branches in milliseconds */
  timeout?: number;
}
