/**
 * Workflow Execution Checkpoint Adapter
 * Encapsulates SDK API calls related to Workflow Execution Checkpoints
 */

import { BaseAdapter } from "./base-adapter.js";
import { CheckpointResourceAPI } from "@wf-agent/sdk";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Workflow Execution Checkpoint Adapter
 */
export class WorkflowExecutionCheckpointAdapter extends BaseAdapter {
  private checkpointAPI: CheckpointResourceAPI;

  constructor() {
    super();
    this.checkpointAPI = new CheckpointResourceAPI();
  }

  /**
   * Create workflow execution checkpoint
   * @param executionId Execution ID
   * @param name Checkpoint name
   */
  async createCheckpoint(executionId: string, name?: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = {
        id: `checkpoint-${Date.now()}`,
        executionId: executionId,
        workflowId: "default", // Default workflow ID, which should be obtained from the execution during actual use.
        timestamp: Date.now(),
        metadata: {
          name: name || `Checkpoint ${new Date().toISOString()}`,
          description: "Manually created checkpoint",
        },
      };

      await this.checkpointAPI.create(checkpoint);
      this.output.infoLog(`Checkpoint created: ${checkpoint.id}`);
      return checkpoint;
    }, "Create a workflow execution checkpoint");
  }

  /**
   * Restore workflow execution from checkpoint
   * @param checkpointId Checkpoint ID
   */
  async loadCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.checkpointAPI.get(checkpointId);
      const checkpoint = (result as any).data || result;

      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "WorkflowExecutionCheckpoint",
          checkpointId,
        );
      }

      // Restore the checkpoint
      await this.checkpointAPI.restoreFromCheckpoint(checkpointId);
      this.output.infoLog(`Workflow execution restored from checkpoint: ${checkpointId}`);
    }, "Load workflow execution checkpoint");
  }

  /**
   * List all workflow execution checkpoints
   * @param filter Filter conditions
   */
  async listCheckpoints(filter?: any): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.checkpointAPI.getAll();
      const checkpoints = (result as any).data || result;

      // **Summary Format**
      const summaries = checkpoints.map((cp: any) => ({
        id: cp.id,
        executionId: cp.executionId,
        timestamp: cp.timestamp,
        metadata: cp.metadata,
      }));

      return summaries;
    }, "List workflow execution checkpoints");
  }

  /**
   * Get workflow execution checkpoint details
   * @param checkpointId Checkpoint ID
   */
  async getCheckpoint(checkpointId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.checkpointAPI.get(checkpointId);
      const checkpoint = (result as any).data || result;

      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "WorkflowExecutionCheckpoint",
          checkpointId,
        );
      }

      return checkpoint;
    }, "Get workflow execution checkpoint details");
  }

  /**
   * Delete workflow execution checkpoint
   * @param checkpointId Checkpoint ID
   */
  async deleteCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.checkpointAPI.delete(checkpointId);

      this.output.infoLog(`Checkpoint deleted: ${checkpointId}`);
    }, "Delete workflow execution checkpoint");
  }

  /**
   * Create workflow execution checkpoint (using API method)
   * @param executionId Execution ID
   * @param metadata Checkpoint metadata
   */
  async createWorkflowExecutionCheckpoint(executionId: string, metadata?: any): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const checkpointId = await this.checkpointAPI.createWorkflowExecutionCheckpoint(executionId, metadata);
      this.output.infoLog(`Workflow execution checkpoint created: ${checkpointId}`);
      return checkpointId;
    }, "Create a workflow execution checkpoint");
  }

  /**
   * Restore workflow execution from checkpoint (using API method)
   * @param checkpointId Checkpoint ID
   */
  async restoreFromCheckpoint(checkpointId: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const executionId = await this.checkpointAPI.restoreFromCheckpoint(checkpointId);
      this.output.infoLog(`Workflow execution restored from checkpoint: ${checkpointId}`);
      return executionId;
    }, "Restore workflow execution from checkpoint");
  }

  /**
   * Get checkpoint list for a workflow execution
   * @param executionId Execution ID
   */
  async getWorkflowExecutionCheckpoints(executionId: string): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      // Get all checkpoints and filter by executionId
      const allCheckpoints = await this.checkpointAPI.getAll();
      const checkpoints = (allCheckpoints as any).data || allCheckpoints;
      return checkpoints.filter((cp: any) => cp.executionId === executionId);
    }, "Get workflow execution checkpoint list");
  }

  /**
   * Get the latest checkpoint for a workflow execution
   * @param executionId Execution ID
   */
  async getLatestCheckpoint(executionId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = await this.checkpointAPI.getLatestCheckpoint(executionId);
      if (!checkpoint) {
        throw new Error(`No checkpoint found for workflow execution: ${executionId}`);
      }
      return checkpoint;
    }, "Get the latest workflow execution checkpoint");
  }

  /**
   * Get checkpoint statistics
   */
  async getStatistics(): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const stats = await this.checkpointAPI.getCheckpointStatistics();
      return stats;
    }, "Get workflow execution checkpoint statistics");
  }
}
