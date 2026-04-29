/**
 * Thread Checkpoint Adapter
 * Encapsulates SDK API calls related to Thread (Workflow) Checkpoints
 */

import { BaseAdapter } from "./base-adapter.js";
import { CheckpointResourceAPI } from "@wf-agent/sdk";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Thread Checkpoint Adapter
 */
export class ThreadCheckpointAdapter extends BaseAdapter {
  private checkpointAPI: CheckpointResourceAPI;

  constructor() {
    super();
    this.checkpointAPI = new CheckpointResourceAPI();
  }

  /**
   * Create Thread checkpoint
   * @param threadId Thread ID
   * @param name Checkpoint name
   */
  async createCheckpoint(threadId: string, name?: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = {
        id: `checkpoint-${Date.now()}`,
        executionId: threadId,
        workflowId: "default", // Default workflow ID, which should be obtained from the Thread during actual use.
        timestamp: Date.now(),
        metadata: {
          name: name || `Checkpoint ${new Date().toISOString()}`,
          description: "Manually created checkpoint",
        },
      };

      await this.checkpointAPI.create(checkpoint);
      this.output.infoLog(`Checkpoint created: ${checkpoint.id}`);
      return checkpoint;
    }, "Create a Thread checkpoint");
  }

  /**
   * Restore Thread from checkpoint
   * @param checkpointId Checkpoint ID
   */
  async loadCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.checkpointAPI.get(checkpointId);
      const checkpoint = (result as any).data || result;

      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "ThreadCheckpoint",
          checkpointId,
        );
      }

      // Restore the checkpoint
      await this.checkpointAPI.restoreFromCheckpoint(checkpointId);
      this.output.infoLog(`Thread restored from checkpoint: ${checkpointId}`);
    }, "Load Thread Checkpoint");
  }

  /**
   * List all Thread checkpoints
   * @param filter Filter conditions
   */
  async listCheckpoints(filter?: any): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.checkpointAPI.getAll();
      const checkpoints = (result as any).data || result;

      // **Summary Format**
      const summaries = checkpoints.map((cp: any) => ({
        id: cp.id,
        threadId: cp.threadId,
        timestamp: cp.timestamp,
        metadata: cp.metadata,
      }));

      return summaries;
    }, "List Thread Checkpoints");
  }

  /**
   * Get Thread checkpoint details
   * @param checkpointId Checkpoint ID
   */
  async getCheckpoint(checkpointId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.checkpointAPI.get(checkpointId);
      const checkpoint = (result as any).data || result;

      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "ThreadCheckpoint",
          checkpointId,
        );
      }

      return checkpoint;
    }, "Get Thread checkpoint details");
  }

  /**
   * Delete Thread checkpoint
   * @param checkpointId Checkpoint ID
   */
  async deleteCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.checkpointAPI.delete(checkpointId);

      this.output.infoLog(`Checkpoint deleted: ${checkpointId}`);
    }, "Delete Thread checkpoint");
  }

  /**
   * Create Thread checkpoint (using API method)
   * @param threadId Thread ID
   * @param metadata Checkpoint metadata
   */
  async createThreadCheckpoint(threadId: string, metadata?: any): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const checkpointId = await this.checkpointAPI.createThreadCheckpoint(threadId, metadata);
      this.output.infoLog(`Thread checkpoint created: ${checkpointId}`);
      return checkpointId;
    }, "Create a Thread checkpoint");
  }

  /**
   * Restore Thread from checkpoint (using API method)
   * @param checkpointId Checkpoint ID
   */
  async restoreFromCheckpoint(checkpointId: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const threadId = await this.checkpointAPI.restoreFromCheckpoint(checkpointId);
      this.output.infoLog(`Thread restored from checkpoint: ${checkpointId}`);
      return threadId;
    }, "Restore Thread from checkpoint");
  }

  /**
   * Get checkpoint list for a Thread
   * @param threadId Thread ID
   */
  async getThreadCheckpoints(threadId: string): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const checkpoints = await this.checkpointAPI.getThreadCheckpoints(threadId);
      return checkpoints;
    }, "Get Thread checkpoint list");
  }

  /**
   * Get the latest checkpoint for a Thread
   * @param threadId Thread ID
   */
  async getLatestCheckpoint(threadId: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = await this.checkpointAPI.getLatestCheckpoint(threadId);
      if (!checkpoint) {
        throw new Error(`No checkpoint found for Thread: ${threadId}`);
      }
      return checkpoint;
    }, "Get the latest Thread checkpoint");
  }

  /**
   * Get checkpoint statistics
   */
  async getStatistics(): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const stats = await this.checkpointAPI.getCheckpointStatistics();
      return stats;
    }, "Get Thread checkpoint statistics");
  }
}
