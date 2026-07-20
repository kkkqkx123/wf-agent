/**
 * Workflow Execution Checkpoint Adapter
 * Manage checkpoints for workflow executions.
 */

import { BaseAdapter } from "./base-adapter.js";
import { CheckpointResourceAPI } from "@wf-agent/sdk/api";
import type { CheckpointFilter } from "@wf-agent/sdk/api";

export class WorkflowExecutionCheckpointAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Checkpoint";
  }

  private getCheckpointAPI(): CheckpointResourceAPI {
    const deps = this.sdk.getFactory().getDependencies();
    return new CheckpointResourceAPI(deps);
  }

  /**
   * Create a checkpoint for an execution
   */
  async createCheckpoint(executionId: string, name?: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("createCheckpoint", { executionId, name });
      const api = this.getCheckpointAPI();
      const metadata = name ? { description: name } : undefined;
      const checkpointId = await api.createWorkflowExecutionCheckpoint(executionId, metadata);
      return {
        id: checkpointId,
        executionId,
        name: name || `checkpoint-${Date.now()}`,
        createdAt: new Date().toISOString(),
      };
    }, "Create checkpoint");
  }

  /**
   * Load (restore) a checkpoint
   */
  async loadCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("loadCheckpoint", { checkpointId });
      const api = this.getCheckpointAPI();
      await api.restoreFromCheckpoint(checkpointId);
    }, "Load checkpoint");
  }

  /**
   * List all checkpoints
   */
  async listCheckpoints(filter?: Record<string, unknown>): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listCheckpoints", filter);
      const api = this.getCheckpointAPI();
      const checkpointFilter: CheckpointFilter = {};
      if (filter?.["executionId"]) {
        (checkpointFilter as any).executionId = filter["executionId"] as string;
      }

      const checkpoints = await api.getAll(checkpointFilter);
      return checkpoints.map((cp: any) => ({
        id: cp.id,
        executionId: cp.executionId,
        workflowId: cp.workflowId,
        name: cp.metadata?.description || cp.id,
        timestamp: cp.timestamp,
        tags: cp.metadata?.tags,
      }));
    }, "List checkpoints");
  }

  /**
   * Get checkpoint details
   */
  async getCheckpoint(checkpointId: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getCheckpoint", { checkpointId });
      const api = this.getCheckpointAPI();
      const checkpoint = await api.get(checkpointId);
      if (!checkpoint) {
        throw new Error(`Checkpoint not found: ${checkpointId}`);
      }
      return {
        id: checkpoint.id,
        executionId: checkpoint.executionId,
        workflowId: checkpoint.workflowId,
        timestamp: checkpoint.timestamp,
        metadata: checkpoint.metadata,
        type: (checkpoint as any).type,
        previousCheckpointId: (checkpoint as any).previousCheckpointId,
      };
    }, "Get checkpoint");
  }

  /**
   * Delete a checkpoint
   */
  async deleteCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("deleteCheckpoint", { checkpointId });
      const api = this.getCheckpointAPI();
      await api.delete(checkpointId);
    }, "Delete checkpoint");
  }
}