/**
 * Workflow Execution Adapter
 * Encapsulates SDK API calls related to workflow executions
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";
import { getData, isFailure, getError } from "@wf-agent/sdk";
import {
  ExecuteWorkflowCommand,
  PauseWorkflowCommand,
  ResumeWorkflowCommand,
  CancelWorkflowCommand,
} from "@wf-agent/sdk";

/**
 * Workflow Execution Adapter
 */
export class WorkflowExecutionAdapter extends BaseAdapter {
  /**
   * Execute workflow
   */
  async executeWorkflow(workflowId: string, input?: Record<string, unknown>): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      // Create and execute the command using SDK's command execution interface
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new ExecuteWorkflowCommand({ workflowId, options: { input } }, dependencies);
      const result = await this.sdk.executeCommand(command);

      if (isFailure(result)) {
        throw getError(result);
      }

      const executionResult = getData(result);
      this.output.infoLog(`Workflow execution started successfully`);
      return executionResult;
    }, "Execute workflow");
  }

  /**
   * Pause workflow execution
   */
  async pauseWorkflowExecution(executionId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Create and execute pause command
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new PauseWorkflowCommand(executionId, dependencies);
      const result = await this.sdk.executeCommand(command);
      
      if (isFailure(result)) {
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
      const command = new ResumeWorkflowCommand(executionId, dependencies);
      const result = await this.sdk.executeCommand(command);
      
      if (isFailure(result)) {
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
      const command = new CancelWorkflowCommand(executionId, dependencies);
      const result = await this.sdk.executeCommand(command);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      this.output.infoLog(`Workflow execution stopped: ${executionId}`);
    }, "Stop workflow execution");
  }

  /**
   * List all workflow executions
   */
  async listWorkflowExecutions(filter?: Record<string, unknown>): Promise<Array<{
    id: string;
    workflowId: string;
    status: string;
    createdAt: string;
    updatedAt: string;
  }>> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      const result = await api.getAll();
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const executions = getData(result) as unknown as Array<{
        id: string;
        workflowId: string;
        status: string;
        createdAt?: string;
        updatedAt?: string;
      }>;

      // Conversion to summary format
      const summaries = executions.map((execution) => ({
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
  async getWorkflowExecution(executionId: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      const result = await api.get(executionId);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const execution = getData(result);

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
