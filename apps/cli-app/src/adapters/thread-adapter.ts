/**
 * Thread Adapter
 * Encapsulates SDK API calls related to threads
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Thread Adapter
 */
export class ThreadAdapter extends BaseAdapter {
  /**
   * Execute workflow thread
   */
  async executeThread(workflowId: string, input?: Record<string, unknown>): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      // Using the SDK's Execution Methods
      const result = await (this.sdk as any).execute(workflowId, input || {});

      this.output.infoLog(`Thread executed successfully`);
      return result;
    }, "Execute thread");
  }

  /**
   * Pause thread
   */
  async pauseThread(threadId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Using the SDK's pause method
      await (this.sdk as any).pause(threadId);
      this.output.infoLog(`Thread paused: ${threadId}`);
    }, "Pause thread");
  }

  /**
   * Resume thread
   */
  async resumeThread(threadId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Resume methods using the SDK
      await (this.sdk as any).resume(threadId);
      this.output.infoLog(`Thread resumed: ${threadId}`);
    }, "Resume thread");
  }

  /**
   * Stop thread
   */
  async stopThread(threadId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      // Stop methods using the SDK
      await (this.sdk as any).cancel(threadId);
      this.output.infoLog(`Thread stopped: ${threadId}`);
    }, "Stop thread");
  }

  /**
   * List all workflow executions
   */
  async listThreads(filter?: any): Promise<any[]> {
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
  async getThread(threadId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      const result = await api.get(threadId);
      const execution = (result as any).data || result;

      if (!execution) {
        throw new CLINotFoundError(`Workflow execution not found: ${threadId}`, "WorkflowExecution", threadId);
      }

      return execution;
    }, "Get workflow execution details");
  }

  /**
   * Delete workflow execution
   */
  async deleteThread(threadId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.executions;
      await api.delete(threadId);

      this.output.infoLog(`Workflow execution deleted: ${threadId}`);
    }, "Delete workflow execution");
  }
}
