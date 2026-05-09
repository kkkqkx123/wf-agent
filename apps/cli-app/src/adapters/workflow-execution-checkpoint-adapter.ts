/**
 * Workflow Execution Checkpoint Adapter
 * Encapsulates SDK API calls related to Workflow Execution Checkpoints
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";
import { getData, isFailure, getError } from "@wf-agent/sdk";

/**
 * Workflow Execution Checkpoint Adapter
 */
export class WorkflowExecutionCheckpointAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Get checkpoint API instance
   */
  private getCheckpointAPI(): any {
    return (this.sdk as any).getWorkflowExecutionAPI().checkpoint;
  }

  /**
   * Create workflow execution checkpoint
   * @param executionId Execution ID
   * @param name Checkpoint name
   */
  async createCheckpoint(executionId: string, name?: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      // Use the proper API method to create a checkpoint
      const checkpointId = await this.getCheckpointAPI().createWorkflowExecutionCheckpoint(executionId, {
        description: name || `Checkpoint ${new Date().toISOString()}`,
      });
      
      this.output.infoLog(`Checkpoint created: ${checkpointId}`);
      
      // Return the created checkpoint details
      const checkpoint = await this.getCheckpointAPI().get(checkpointId);
      return checkpoint;
    }, "Create a workflow execution checkpoint");
  }

  /**
   * Restore workflow execution from checkpoint
   * @param checkpointId Checkpoint ID
   */
  async loadCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.getCheckpointAPI().get(checkpointId);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const checkpoint = getData(result);

      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "WorkflowExecutionCheckpoint",
          checkpointId,
        );
      }

      // Restore the checkpoint
      await this.getCheckpointAPI().restoreFromCheckpoint(checkpointId);
      this.output.infoLog(`Workflow execution restored from checkpoint: ${checkpointId}`);
    }, "Load workflow execution checkpoint");
  }

  /**
   * List all workflow execution checkpoints
   * @param filter Filter conditions
   */
  async listCheckpoints(filter?: Record<string, unknown>): Promise<Array<{
    id: string;
    executionId: string;
    timestamp: string;
    metadata?: unknown;
  }>> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.getCheckpointAPI().getAll();
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const checkpoints = getData(result) as unknown as Array<{
        id: string;
        executionId: string;
        timestamp: string;
        metadata?: unknown;
      }>;

      // **Summary Format**
      const summaries = checkpoints.map((cp) => ({
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
  async getCheckpoint(checkpointId: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.getCheckpointAPI().get(checkpointId);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const checkpoint = getData(result);

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
      await this.getCheckpointAPI().delete(checkpointId);

      this.output.infoLog(`Checkpoint deleted: ${checkpointId}`);
    }, "Delete workflow execution checkpoint");
  }

  /**
   * Create workflow execution checkpoint (using API method)
   * @param executionId Execution ID
   * @param metadata Checkpoint metadata
   */
  async createWorkflowExecutionCheckpoint(executionId: string, metadata?: Record<string, unknown>): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const checkpointId = await this.getCheckpointAPI().createWorkflowExecutionCheckpoint(executionId, metadata);
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
      const executionId = await this.getCheckpointAPI().restoreFromCheckpoint(checkpointId);
      this.output.infoLog(`Workflow execution restored from checkpoint: ${checkpointId}`);
      return executionId;
    }, "Restore workflow execution from checkpoint");
  }

  /**
   * Get checkpoint list for a workflow execution
   * @param executionId Execution ID
   */
  async getWorkflowExecutionCheckpoints(executionId: string): Promise<unknown[]> {
    return this.executeWithErrorHandling(async () => {
      // Get all checkpoints and filter by executionId
      const result = await this.getCheckpointAPI().getAll();
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const allCheckpoints = getData(result) as unknown as Array<{ executionId: string }>;
      return allCheckpoints.filter((cp) => cp.executionId === executionId);
    }, "Get workflow execution checkpoint list");
  }

  /**
   * Get the latest checkpoint for a workflow execution
   * @param executionId Execution ID
   */
  async getLatestCheckpoint(executionId: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = await this.getCheckpointAPI().getLatestCheckpoint(executionId);
      if (!checkpoint) {
        throw new Error(`No checkpoint found for workflow execution: ${executionId}`);
      }
      return checkpoint;
    }, "Get the latest workflow execution checkpoint");
  }

  /**
   * Get checkpoint statistics
   */
  async getStatistics(): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const stats = await this.getCheckpointAPI().getCheckpointStatistics();
      return stats;
    }, "Get workflow execution checkpoint statistics");
  }
}
