/**
 * Workflow Execution Checkpoint Adapter
 * Encapsulates SDK API calls related to Workflow Execution Checkpoints
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";
import { CheckpointResourceAPI } from "@wf-agent/sdk/api";
import type { Checkpoint, WorkflowCheckpointTriggerType } from "@wf-agent/types";

/**
 * Type alias for checkpoint with createdAt field (matches formatter expectations)
 */
type CheckpointWithMetadata = Checkpoint & {
  createdAt?: string | number;
};

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
  private getCheckpointAPI(): CheckpointResourceAPI {
    const deps = this.sdk.getFactory().getDependencies();
    return new CheckpointResourceAPI(deps);
  }

  /**
   * Create workflow execution checkpoint
   * @param executionId Execution ID
   * @param name Checkpoint name
   */
  async createCheckpoint(executionId: string, name?: string): Promise<CheckpointWithMetadata> {
    return this.executeWithErrorHandling(async () => {
      // Use the proper API method to create a checkpoint
      const checkpointId = await this.getCheckpointAPI().createWorkflowExecutionCheckpoint(executionId, {
        description: name || `Checkpoint ${new Date().toISOString()}`,
      });

      this.output.infoLog(`Checkpoint created: ${checkpointId}`);

      // Return the created checkpoint details
      const checkpoint = await this.getCheckpointAPI().get(checkpointId);

      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "WorkflowExecutionCheckpoint",
          checkpointId,
        );
      }

      // Add createdAt field for formatter compatibility
      return {
        ...checkpoint,
        createdAt: checkpoint.timestamp,
      } as CheckpointWithMetadata;
    }, "Create a workflow execution checkpoint");
  }

  /**
   * Restore workflow execution from checkpoint
   * @param checkpointId Checkpoint ID
   */
  async loadCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = await this.getCheckpointAPI().get(checkpointId);

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
  async listCheckpoints(filter?: Record<string, unknown>): Promise<CheckpointWithMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      // Convert filter to CheckpointFilter type
      const checkpointFilter = filter
        ? {
            ids: (filter["ids"] as string[] | undefined),
            executionId: (filter["executionId"] as string | undefined),
            workflowId: (filter["workflowId"] as string | undefined),
            triggerType: (filter["triggerType"] as WorkflowCheckpointTriggerType | undefined),
            creator: (filter["creator"] as string | undefined),
            tags: (filter["tags"] as string[] | undefined),
          }
        : undefined;

      const checkpoints = await this.getCheckpointAPI().getAll(checkpointFilter);

      // Convert to CheckpointWithMetadata format
      return checkpoints.map((cp) => ({
        ...cp,
        createdAt: cp.timestamp,
      })) as CheckpointWithMetadata[];
    }, "List workflow execution checkpoints");
  }

  /**
   * Get workflow execution checkpoint details
   * @param checkpointId Checkpoint ID
   */
  async getCheckpoint(checkpointId: string): Promise<CheckpointWithMetadata> {
    return this.executeWithErrorHandling(async () => {
      const checkpoint = await this.getCheckpointAPI().get(checkpointId);

      if (!checkpoint) {
        throw new CLINotFoundError(
          `Checkpoint not found: ${checkpointId}`,
          "WorkflowExecutionCheckpoint",
          checkpointId,
        );
      }

      // Add createdAt field for formatter compatibility
      return {
        ...checkpoint,
        createdAt: checkpoint.timestamp,
      } as CheckpointWithMetadata;
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
      const allCheckpoints = await this.getCheckpointAPI().getAll();

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

  /**
   * Query checkpoints by filter
   * @param filter Filter criteria
   */
  async queryCheckpoints(filter?: Record<string, unknown>): Promise<CheckpointWithMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const checkpointFilter: any = filter
        ? {
            ids: (filter["ids"] as string[] | undefined),
            executionId: (filter["executionId"] as string | undefined),
            workflowId: (filter["workflowId"] as string | undefined),
            triggerType: (filter["triggerType"] as WorkflowCheckpointTriggerType | undefined),
            creator: (filter["creator"] as string | undefined),
            tags: (filter["tags"] as string[] | undefined),
            timestampRange: filter["timestampRange"] as
              | { start?: number; end?: number }
              | undefined,
          }
        : {};

      const checkpoints = await this.getCheckpointAPI().query(checkpointFilter);
      return checkpoints.map((cp) => ({
        ...cp,
        createdAt: cp.timestamp,
      })) as CheckpointWithMetadata[];
    }, "Query workflow execution checkpoints with filters");
  }

  /**
   * Get checkpoints within time range
   * @param executionId Execution ID
   * @param startTime Start timestamp (ms)
   * @param endTime End timestamp (ms)
   */
  async getCheckpointsByTimeRange(
    executionId: string,
    startTime: number,
    endTime: number,
  ): Promise<CheckpointWithMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const checkpoints = await this.getCheckpointAPI().getByTimeRange(executionId, startTime, endTime);
      return checkpoints.map((cp) => ({
        ...cp,
        createdAt: cp.timestamp,
      })) as CheckpointWithMetadata[];
    }, "Get workflow execution checkpoints by time range");
  }

  /**
   * Get workflow checkpoints within time range
   * @param workflowId Workflow ID
   * @param startTime Start timestamp (ms)
   * @param endTime End timestamp (ms)
   */
  async getWorkflowCheckpointsByTimeRange(
    workflowId: string,
    startTime: number,
    endTime: number,
  ): Promise<CheckpointWithMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const checkpoints = await this.getCheckpointAPI().getWorkflowCheckpointsByTimeRange(
        workflowId,
        startTime,
        endTime,
      );
      return checkpoints.map((cp) => ({
        ...cp,
        createdAt: cp.timestamp,
      })) as CheckpointWithMetadata[];
    }, "Get workflow checkpoints by time range");
  }

  /**
   * Get checkpoints by tags
   * @param executionId Execution ID
   * @param tags Tag array
   */
  async getCheckpointsByTags(executionId: string, tags: string[]): Promise<CheckpointWithMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const checkpoints = await this.getCheckpointAPI().getByTags(executionId, tags);
      return checkpoints.map((cp) => ({
        ...cp,
        createdAt: cp.timestamp,
      })) as CheckpointWithMetadata[];
    }, "Get workflow execution checkpoints by tags");
  }

  /**
   * Get checkpoints by IDs
   * @param ids Checkpoint ID list
   */
  async getCheckpointsById(ids: string[]): Promise<CheckpointWithMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const checkpoints = await this.getCheckpointAPI().getByIds(ids);
      return checkpoints.map((cp) => ({
        ...cp,
        createdAt: cp.timestamp,
      })) as CheckpointWithMetadata[];
    }, "Get workflow execution checkpoints by IDs");
  }

  /**
   * Get checkpoint chain analysis for an execution
   * @param executionId Execution ID
   */
  async getCheckpointChain(executionId: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const chain = await this.getCheckpointAPI().getCheckpointChain(executionId);
      return chain;
    }, "Get workflow execution checkpoint chain");
  }

  /**
   * Get checkpoint chain from a specific checkpoint
   * @param checkpointId Checkpoint ID
   */
  async getCheckpointChainFrom(checkpointId: string): Promise<CheckpointWithMetadata[]> {
    return this.executeWithErrorHandling(async () => {
      const chain = await this.getCheckpointAPI().getCheckpointChainFrom(checkpointId);
      return chain.map((cp) => ({
        ...cp,
        createdAt: cp.timestamp,
      })) as CheckpointWithMetadata[];
    }, "Get workflow execution checkpoint chain from a specific checkpoint");
  }
}
