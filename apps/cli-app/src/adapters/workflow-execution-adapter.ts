/**
 * Workflow Execution Adapter
 * Encapsulates SDK API calls related to workflow executions
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Workflow Execution Adapter
 */
export class WorkflowExecutionAdapter extends BaseAdapter {
  /**
   * Execute workflow
   */
  async executeWorkflow(workflowId: string, input?: Record<string, unknown>): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      // Using the SDK's Execution Methods
      const result = await (this.sdk as any).execute(workflowId, input || {});

      this.output.infoLog(`Workflow execution started successfully`);
      return result;
    }, "Execute workflow");
  }

  /**
   * Pause workflow execution
   */
  async pauseWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Using the SDK's pause method
      await (this.sdk as any).pause(executionId);
      this.output.infoLog(`Workflow execution paused: ${executionId}`);
    }, "Pause workflow execution");
  }

  /**
   * Resume workflow execution
   */
  async resumeWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Resume methods using the SDK
      await (this.sdk as any).resume(executionId);
      this.output.infoLog(`Workflow execution resumed: ${executionId}`);
    }, "Resume workflow execution");
  }

  /**
   * Stop workflow execution
   */
  async stopWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Stop methods using the SDK
      await (this.sdk as any).cancel(executionId);
      this.output.infoLog(`Workflow execution stopped: ${executionId}`);
    }, "Stop workflow execution");
  }

  /**
   * List all workflow executions
   */
  async listWorkflowExecutions(filter?: any): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      const result = await api.getAll();
      const executions = (result as any).data || result;

      // Conversion to summary format
      const summaries = (executions as any[]).map((execution: any) => ({
        id: execution.id,
        workflowId: execution.workflowId,
        status: execution.status,
        createdAt: execution.createdAt || new Date().toISOString(),
        updatedAt: execution.updatedAt || new Date().toISOString(),
      }));

      return summaries;
    }, "List workflow executions");
  }

  /**
   * Get workflow execution details
   */
  async getWorkflowExecution(executionId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      const result = await api.get(executionId);
      const execution = (result as any).data || result;

      if (!execution) {
        throw new CLINotFoundError(`Workflow execution not found: ${executionId}`, "WorkflowExecution", executionId);
      }

      return execution;
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
