/**
 * Workflow Execution Adapter
 * Encapsulates SDK API calls related to workflow executions
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";
import {
  ExecuteWorkflowCommand,
  PauseWorkflowCommand,
  ResumeWorkflowCommand,
  CancelWorkflowCommand,
  isSuccess,
  getData,
  getError,
} from "@wf-agent/sdk/api";
import type { WorkflowExecution, WorkflowExecutionResult, WorkflowExecutionStatus } from "@wf-agent/types";

/**
 * Type alias for workflow summary (matches formatter expectations)
 */
type WorkflowSummary = (WorkflowExecution | WorkflowExecutionResult) & {
  name?: string;
  status?: string;
  createdAt?: string | number;
};

/**
 * Workflow Execution Adapter
 */
export class WorkflowExecutionAdapter extends BaseAdapter {
  /**
   * Execute workflow
   */
  async executeWorkflow(workflowId: string, input?: Record<string, unknown>): Promise<WorkflowSummary> {
    return this.executeWithErrorHandling(async () => {
      // Create and execute the command using SDK's command execution interface
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new ExecuteWorkflowCommand({ workflowId, options: { input } }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        throw getError(result);
      }

      const executionResult = getData(result);
      if (!executionResult) {
        throw new Error("Workflow execution result is null");
      }

      this.output.infoLog(`Workflow execution started successfully`);

      // Convert to WorkflowSummary format
      return {
        ...executionResult,
        id: executionResult.executionId,
        createdAt: executionResult.metadata.startTime || Date.now(),
        status: executionResult.metadata.status,
      } as WorkflowSummary;
    }, "Execute workflow");
  }

  /**
   * Pause workflow execution
   */
  async pauseWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Create and execute pause command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new PauseWorkflowCommand({ executionId }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        throw getError(result);
      }

      this.output.infoLog(`Workflow execution paused: ${executionId}`);
    }, "Pause workflow execution");
  }

  /**
   * Resume workflow execution
   */
  async resumeWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Create and execute resume command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new ResumeWorkflowCommand({ executionId }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        throw getError(result);
      }

      this.output.infoLog(`Workflow execution resumed: ${executionId}`);
    }, "Resume workflow execution");
  }

  /**
   * Stop workflow execution
   */
  async stopWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Create and execute cancel command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new CancelWorkflowCommand({ executionId }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (!isSuccess(result)) {
        throw getError(result);
      }

      this.output.infoLog(`Workflow execution stopped: ${executionId}`);
    }, "Stop workflow execution");
  }

  /**
   * List all workflow executions
   */
  async listWorkflowExecutions(filter?: Record<string, unknown>): Promise<WorkflowSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;

      // Convert filter to WorkflowExecutionFilter type
      const executionFilter = filter
        ? {
            ids: (filter["ids"] as string[] | undefined),
            workflowId: (filter["workflowId"] as string | undefined),
            status: (filter["status"] as WorkflowExecutionStatus | undefined),
            executionType: (filter["executionType"] as "MAIN" | "FORK_JOIN" | "TRIGGERED_SUBWORKFLOW" | undefined),
          }
        : undefined;

      const executions = await api.getAll(executionFilter);

      // Convert to WorkflowSummary format
      return (executions as WorkflowExecution[]).map((execution) => {
        // Try to get startTime from execution if available
        const execWithTime = execution as WorkflowExecution & { startTime?: number };
        return {
          ...execution,
          createdAt: execWithTime.startTime || Date.now(),
        };
      }) as WorkflowSummary[];
    }, "List workflow executions");
  }

  /**
   * Get workflow execution details
   */
  async getWorkflowExecution(executionId: string): Promise<WorkflowSummary> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      const execution = await api.get(executionId);

      if (!execution) {
        throw new CLINotFoundError(`Workflow execution not found: ${executionId}`, "WorkflowExecution", executionId);
      }

      // Convert to WorkflowSummary format
      const execWithTime = execution as WorkflowExecution & { startTime?: number };
      return {
        ...execution,
        createdAt: execWithTime.startTime || Date.now(),
      } as WorkflowSummary;
    }, "Get workflow execution details");
  }

  /**
   * Delete workflow execution
   */
  async deleteWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      await api.delete(executionId);

      this.output.infoLog(`Workflow execution deleted: ${executionId}`);
    }, "Delete workflow execution");
  }
}
