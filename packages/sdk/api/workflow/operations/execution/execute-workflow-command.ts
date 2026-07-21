/**
 * ExecuteWorkflowCommand - Execute Workflow Command
 *
 * Category: Execution
 * Long-running workflow execution with full lifecycle management
 */

import {
  ExecutionCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateWorkflowExecutionParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { WorkflowExecutionResult, WorkflowExecutionOptions } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import { withExecutionTimeout } from "../../../shared/utils/timeout-execution.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

/**
 * Execute workflow command parameters
 */
export interface ExecuteWorkflowParams {
  /** Workflow ID (required) */
  workflowId: string;
  /** Execution options */
  options?: WorkflowExecutionOptions;
  /** Optional execution timeout in milliseconds */
  timeoutMs?: number;
}

/**
 * Execute Workflow Command
 * Executes a workflow and returns the execution result
 */
export class ExecuteWorkflowCommand extends ExecutionCommand<WorkflowExecutionResult> {
  constructor(
    private readonly params: ExecuteWorkflowParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ExecuteWorkflowCommand",
      description: "Execute a workflow by ID with optional execution parameters",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<WorkflowExecutionResult> {
    const logger = createContextualLogger({
      component: "ExecuteWorkflowCommand",
      commandName: "ExecuteWorkflowCommand",
    });

    const startTime = Date.now();
    const estimatedDefaultTimeout = 300000; // 5 minutes default for workflow execution

    logger.info("Command execution started", {
      workflowId: this.params.workflowId,
      timeoutMs: this.params.timeoutMs ?? estimatedDefaultTimeout,
    });

    try {
      // Obtain the WorkflowLifecycleCoordinator through APIDependencyManager
      const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();

      // Execute workflow with timeout support
      const result = await withExecutionTimeout(
        lifecycleCoordinator.execute(
          this.params.workflowId,
          this.params.options || {},
        ),
        this.params.timeoutMs ?? estimatedDefaultTimeout,
        "Workflow Execution",
      );

      const duration = Date.now() - startTime;
      logger.info("Command execution completed successfully", undefined, {
        workflowId: this.params.workflowId,
        duration,
      });

      return result;
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMsg = error instanceof Error ? error.message : String(error);

      if (errorMsg.includes("timeout")) {
        logger.warn("Command execution timeout", undefined, {
          workflowId: this.params.workflowId,
          duration,
          timeoutMs: this.params.timeoutMs ?? estimatedDefaultTimeout,
        });
      } else {
        logger.error("Command execution failed", undefined, { duration }, error as Error);
      }

      throw error;
    }
  }

  validate(): CommandValidationResult {
    return validateWorkflowExecutionParams(this.params.workflowId);
  }
}
