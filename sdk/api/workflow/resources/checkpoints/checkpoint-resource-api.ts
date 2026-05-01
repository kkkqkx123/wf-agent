/**
 * CheckpointResourceAPI - Checkpoint Resource Management API
 *  Inherits from GenericResourceAPI, providing unified CRUD operations
 */

import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import { CheckpointState } from "../../../../workflow/checkpoint/checkpoint-state-manager.js";
import type { Checkpoint, CheckpointMetadata } from "@wf-agent/types";
import { CheckpointCoordinator } from "../../../../workflow/checkpoint/checkpoint-coordinator.js";
import { getContainer } from "../../../../core/di/index.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import { getErrorMessage, isSuccess, getData } from "../../../shared/types/execution-result.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { Timestamp } from "@wf-agent/types";
import { WorkflowExecutionStatus } from "@wf-agent/types";
import { GraphCheckpointTriggerType } from "@wf-agent/types";
import { buildCheckpointRestoredEvent } from "../../../../core/utils/event/builders/index.js";

/**
 * Checkpoint Filter
 */
export interface CheckpointFilter {
  /** Checkpoint ID list */
  ids?: string[];
  /** Execution ID */
  executionId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Trigger Type */
  triggerType?: GraphCheckpointTriggerType;
  /** Creator */
  creator?: string;
  /** Tag array */
  tags?: string[];
  /** Create a time range */
  timestampRange?: { start?: Timestamp; end?: Timestamp };
  /** Start time (from) */
  startTimeFrom?: Timestamp;
  /** Start time (until) */
  startTimeTo?: Timestamp;
}

/**
 * Checkpoint Summary
 */
export interface CheckpointSummary {
  /** Checkpoint ID */
  id: string;
  /** Execution ID */
  executionId: string;
  /** Workflow ID */
  workflowId: string;
  /** Workflow Execution Status */
  executionStatus: WorkflowExecutionStatus;
  /** Current node ID */
  currentNodeId: string;
  /** Create a timestamp */
  timestamp: Timestamp;
  /** Checkpoint Description */
  description?: string;
  /** Tag array */
  tags?: string[];
}

/**
 * CheckpointResourceAPI - Checkpoint resource management API
 */
export class CheckpointResourceAPI extends CrudResourceAPI<Checkpoint, string, CheckpointFilter> {
  private stateManager: CheckpointState;
  private eventManager?: EventRegistry;

  constructor(eventManager?: EventRegistry) {
    super();

    // Get the CheckpointState from the DI container.
    const container = getContainer();
    this.stateManager = container.get(Identifiers.CheckpointState) as CheckpointState;
    this.eventManager = eventManager;
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get checkpoints from the registry.
   */
  protected async getResource(id: string): Promise<Checkpoint | null> {
    return this.stateManager.get(id) || null;
  }

  /**
   * Get all checkpoints from the registry.
   */
  protected async getAllResources(): Promise<Checkpoint[]> {
    const checkpointIds = await this.stateManager.list();
    const checkpoints: Checkpoint[] = [];
    for (const checkpointId of checkpointIds) {
      const checkpoint = await this.stateManager.get(checkpointId);
      if (checkpoint) {
        checkpoints.push(checkpoint);
      }
    }
    return checkpoints;
  }

  /**
   * Create a checkpoint
   */
  protected async createResource(checkpoint: Checkpoint): Promise<void> {
    // Checkpoints are created by the coordinator and are stored directly here.
    await this.stateManager.create(checkpoint);
  }

  /**
   * Update checkpoints
   */
  protected async updateResource(id: string, updates: Partial<Checkpoint>): Promise<void> {
    const existing = await this.stateManager.get(id);
    if (!existing) {
      throw new Error(`Checkpoint not found: ${id}`);
    }
    // Type-safe merge: preserve the type-specific properties
    const updated: Checkpoint = { ...existing, ...updates } as Checkpoint;
    await this.stateManager.create(updated);
  }

  /**
   * Delete checkpoints.
   */
  protected async deleteResource(id: string): Promise<void> {
    await this.stateManager.delete(id);
  }

  /**
   * Apply filter criteria
   */
  protected override applyFilter(
    checkpoints: Checkpoint[],
    filter: CheckpointFilter,
  ): Checkpoint[] {
    return checkpoints.filter(cp => {
      if (filter.ids && !filter.ids.some(id => cp.id.includes(id))) {
        return false;
      }
      if (filter.executionId && cp.executionId !== filter.executionId) {
        return false;
      }
      if (filter.workflowId && cp.workflowId !== filter.workflowId) {
        return false;
      }
      if (filter.timestampRange?.start && cp.timestamp < filter.timestampRange.start) {
        return false;
      }
      if (filter.timestampRange?.end && cp.timestamp > filter.timestampRange.end) {
        return false;
      }
      if (filter.startTimeFrom && cp.timestamp < filter.startTimeFrom) {
        return false;
      }
      if (filter.startTimeTo && cp.timestamp > filter.startTimeTo) {
        return false;
      }
      if (filter.tags && cp.metadata?.tags) {
        if (!filter.tags.every(tag => cp.metadata?.tags?.includes(tag))) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Clear all checkpoints.
   */
  protected override async clearResources(): Promise<void> {
    const checkpointIds = await this.stateManager.list();
    for (const checkpointId of checkpointIds) {
      await this.stateManager.delete(checkpointId);
    }
  }

  // ============================================================================
  // Checkpoint-specific method
  // ============================================================================

  /**
   * Create a workflow execution checkpoint
   * @param executionId Execution ID
   * @param metadata Checkpoint metadata
   * @returns Checkpoint ID
   */
  async createWorkflowExecutionCheckpoint(executionId: string, metadata?: CheckpointMetadata): Promise<string> {
    // Obtain global services from the DI container.
    const container = getContainer();
    const executionRegistry = container.get(
      Identifiers.WorkflowExecutionRegistry,
    ) as import("../../../../workflow/stores/workflow-execution-registry.js").WorkflowExecutionRegistry;
    const workflowRegistry = container.get(
      Identifiers.WorkflowRegistry,
    ) as import("../../../../workflow/stores/workflow-registry.js").WorkflowRegistry;
    const graphRegistry = container.get(
      Identifiers.WorkflowGraphRegistry,
    ) as import("../../../../workflow/stores/workflow-graph-registry.js").WorkflowGraphRegistry;

    const dependencies = {
      workflowExecutionRegistry:
        executionRegistry as unknown as import("../../../../workflow/stores/workflow-execution-registry.js").WorkflowExecutionRegistry,
      checkpointStateManager: this.stateManager,
      workflowRegistry,
      workflowGraphRegistry: graphRegistry,
    };

    const checkpointId = await CheckpointCoordinator.createCheckpoint(
      executionId,
      dependencies,
      metadata,
    );
    return checkpointId;
  }

  /**
   * Restore a workflow execution from a checkpoint
   * @param checkpointId Checkpoint ID
   * @returns ID of the restored workflow execution
   */
  async restoreFromCheckpoint(checkpointId: string): Promise<string> {
    // Obtain global services from the DI container.
    const container = getContainer();
    const executionRegistry = container.get(
      Identifiers.WorkflowExecutionRegistry,
    ) as import("../../../../workflow/stores/workflow-execution-registry.js").WorkflowExecutionRegistry;
    const workflowRegistry = container.get(
      Identifiers.WorkflowRegistry,
    ) as import("../../../../workflow/stores/workflow-registry.js").WorkflowRegistry;
    const graphRegistry = container.get(
      Identifiers.WorkflowGraphRegistry,
    ) as import("../../../../workflow/stores/workflow-graph-registry.js").WorkflowGraphRegistry;

    const dependencies = {
      workflowExecutionRegistry:
        executionRegistry as unknown as import("../../../../workflow/stores/workflow-execution-registry.js").WorkflowExecutionRegistry,
      checkpointStateManager: this.stateManager,
      workflowRegistry,
      workflowGraphRegistry: graphRegistry,
    };

    const { workflowExecutionEntity } = await CheckpointCoordinator.restoreFromCheckpoint(
      checkpointId,
      dependencies,
    );

    // Trigger a checkpoint recovery event.
    if (this.eventManager) {
      const checkpoint = await this.stateManager.get(checkpointId);
      if (checkpoint) {
        await this.eventManager.emit(
          buildCheckpointRestoredEvent({
            workflowId: checkpoint.workflowId,
            executionId: workflowExecutionEntity.id,
            checkpointId,
            description: checkpoint.metadata?.description,
          }),
        );
      }
    }

    return workflowExecutionEntity.id;
  }

  /**
   * Get the list of checkpoints for the workflow execution
   * @param executionId: Execution ID
   * @returns: Array of checkpoints
   */
  async getWorkflowExecutionCheckpoints(executionId: string): Promise<Checkpoint[]> {
    const result = await this.getAll({ executionId });
    if (!isSuccess(result)) {
      throw new Error(getErrorMessage(result) || "Failed to get workflow execution checkpoints");
    }
    const checkpoints = getData(result);
    return checkpoints || [];
  }

  /**
   * Get the latest checkpoint
   * @param executionId Execution ID
   * @returns The latest checkpoint; returns null if it does not exist
   */
  async getLatestCheckpoint(executionId: string): Promise<Checkpoint | null> {
    const checkpoints = await this.getWorkflowExecutionCheckpoints(executionId);
    if (checkpoints.length === 0) {
      return null;
    }

    // Sort in descending order by timestamp and return the latest checkpoint.
    const latest = checkpoints.sort((a, b) => b.timestamp - a.timestamp)[0];
    return latest || null;
  }

  /**
   * Get checkpoint statistics information
   * @returns Statistical information
   */
  async getCheckpointStatistics(): Promise<{
    total: number;
    byExecution: Record<string, number>;
    byWorkflow: Record<string, number>;
  }> {
    const result = await this.getAll();
    if (!isSuccess(result)) {
      throw new Error(getErrorMessage(result) || "Failed to get checkpoint statistics");
    }
    const checkpoints = getData(result) || [];

    const byExecution: Record<string, number> = {};
    const byWorkflow: Record<string, number> = {};

    for (const checkpoint of checkpoints) {
      byExecution[checkpoint.executionId] = (byExecution[checkpoint.executionId] || 0) + 1;
      byWorkflow[checkpoint.workflowId] = (byWorkflow[checkpoint.workflowId] || 0) + 1;
    }

    return {
      total: checkpoints.length,
      byExecution,
      byWorkflow,
    };
  }

  /**
   * Get the underlying CheckpointState instance
   * @returns CheckpointState instance
   */
  getStateManager(): CheckpointState {
    return this.stateManager;
  }
}
