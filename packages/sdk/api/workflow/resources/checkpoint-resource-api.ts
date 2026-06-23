/**
 * CheckpointResourceAPI - Checkpoint Resource Management API
 *  Inherits from GenericResourceAPI, providing unified CRUD operations
 */

import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import { CheckpointState } from "../../../workflow/checkpoint/checkpoint-state-manager.js";
import type { Checkpoint, CheckpointMetadata } from "@wf-agent/types";
import { CheckpointCoordinator } from "../../../workflow/checkpoint/checkpoint-coordinator.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import type { Timestamp } from "@wf-agent/types";
import { WorkflowExecutionStatus } from "@wf-agent/types";
import { buildCheckpointRestoredEvent } from "../../../shared/utils/event/builders/index.js";

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
  triggerType?: string;
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
 * Checkpoint transition information
 */
export interface CheckpointTransition {
  /** Previous checkpoint ID */
  fromCheckpointId: string;
  /** Current checkpoint ID */
  toCheckpointId: string;
  /** Time elapsed between checkpoints (ms) */
  elapsed: number;
  /** Status change (if any) */
  statusChange?: { from: string; to: string };
  /** Current node change (if any) */
  currentNodeChange?: { from: string; to: string };
  /** Checkpoint type (FULL or DELTA) */
  type: "FULL" | "DELTA";
  /** Description of the transition trigger */
  triggerDescription?: string;
}

/**
 * Checkpoint chain analysis
 */
export interface CheckpointChainAnalysis {
  /** Execution ID */
  executionId: string;
  /** Checkpoints in chronological order (oldest first) */
  checkpoints: Checkpoint[];
  /** Transitions between consecutive checkpoints */
  transitions: CheckpointTransition[];
  /** Total elapsed time from first to last checkpoint (ms) */
  totalElapsed: number;
  /** Number of checkpoints in the chain */
  checkpointCount: number;
  /** Time range */
  timeRange: { start: Timestamp; end: Timestamp };
}

/**
 * CheckpointResourceAPI - Checkpoint resource management API
 */
export class CheckpointResourceAPI extends SimplifiedCrudResourceAPI<Checkpoint, string, CheckpointFilter> {
  private stateManager: CheckpointState;
  private eventManager?: EventRegistry;
  private deps: APIDependencyManager;

  constructor(deps: APIDependencyManager, eventManager?: EventRegistry) {
    super();

    this.deps = deps;
    // Get the CheckpointState from the DI container.
    this.stateManager = deps.getCheckpointStateManager();
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
  async createWorkflowExecutionCheckpoint(
    executionId: string,
    metadata?: CheckpointMetadata,
  ): Promise<string> {
    const executionRegistry = this.deps.getWorkflowExecutionRegistry();
    const workflowRegistry = this.deps.getWorkflowRegistry();
    const graphRegistry = this.deps.getWorkflowGraphRegistry();

    const dependencies = {
      workflowExecutionRegistry: executionRegistry,
      checkpointStateManager: this.stateManager,
      workflowRegistry,
      workflowGraphRegistry: graphRegistry,
    };

    const checkpointId = await CheckpointCoordinator.createCheckpoint(executionId, dependencies, {
      metadata,
    });
    return checkpointId;
  }

  /**
   * Restore a workflow execution from a checkpoint
   * @param checkpointId Checkpoint ID
   * @returns ID of the restored workflow execution
   */
  async restoreFromCheckpoint(checkpointId: string): Promise<string> {
    const executionRegistry = this.deps.getWorkflowExecutionRegistry();
    const workflowRegistry = this.deps.getWorkflowRegistry();
    const graphRegistry = this.deps.getWorkflowGraphRegistry();

    const dependencies = {
      workflowExecutionRegistry: executionRegistry,
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
    return this.getAll({ executionId });
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
    const checkpoints = await this.getAll();

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

  /**
   * Get checkpoint chain analysis for an execution
   * Builds a chronological chain of checkpoints with transition analysis
   * @param executionId Execution ID
   * @returns Checkpoint chain analysis
   */
  async getCheckpointChain(executionId: string): Promise<CheckpointChainAnalysis> {
    const checkpoints = await this.getWorkflowExecutionCheckpoints(executionId);

    // Sort chronologically (oldest first)
    const sorted = [...checkpoints].sort((a, b) => a.timestamp - b.timestamp);

    const transitions: CheckpointTransition[] = [];
    for (let i = 1; i < sorted.length; i++) {
      const prev = sorted[i - 1]!;
      const curr = sorted[i]!;
      const elapsed = curr.timestamp - prev.timestamp;

      const transition: CheckpointTransition = {
        fromCheckpointId: prev.id,
        toCheckpointId: curr.id,
        elapsed,
        type: curr.type || "FULL",
        triggerDescription: curr.metadata?.description,
      };

      // Extract status change from delta if available
      if (curr.delta?.statusChange) {
        transition.statusChange = {
          from: curr.delta.statusChange.from,
          to: curr.delta.statusChange.to,
        };
      }

      // Extract current node change from delta if available
      if (curr.delta?.currentNodeChange) {
        transition.currentNodeChange = {
          from: curr.delta.currentNodeChange.from,
          to: curr.delta.currentNodeChange.to,
        };
      }

      transitions.push(transition);
    }

    const totalElapsed =
      sorted.length >= 2 ? sorted[sorted.length - 1]!.timestamp - sorted[0]!.timestamp : 0;

    return {
      executionId,
      checkpoints: sorted,
      transitions,
      totalElapsed,
      checkpointCount: sorted.length,
      timeRange: {
        start: sorted.length > 0 ? sorted[0]!.timestamp : 0,
        end: sorted.length > 0 ? sorted[sorted.length - 1]!.timestamp : 0,
      },
    };
  }

  /**
   * Get checkpoint chain starting from a specific checkpoint
   * Follows previousCheckpointId links backwards
   * @param checkpointId Starting checkpoint ID
   * @returns Checkpoints in reverse chronological order (newest first)
   */
  async getCheckpointChainFrom(checkpointId: string): Promise<Checkpoint[]> {
    const chain: Checkpoint[] = [];
    let currentId: string | undefined = checkpointId;

    while (currentId) {
      const checkpoint = await this.stateManager.get(currentId);
      if (!checkpoint) {
        break;
      }
      chain.push(checkpoint);
      currentId = checkpoint.previousCheckpointId;
    }

    return chain;
  }

  /**
   * Query checkpoints by filter
   * @param filter Filter criteria
   * @returns Filtered checkpoints
   */
  async query(filter: CheckpointFilter): Promise<Checkpoint[]> {
    const all = await this.getAll();
    return this.applyFilter(all, filter);
  }

  /**
   * Get checkpoints in time range
   * @param executionId Execution ID
   * @param startTime Start timestamp (ms)
   * @param endTime End timestamp (ms)
   * @returns Checkpoints in time range
   */
  async getByTimeRange(
    executionId: string,
    startTime: Timestamp,
    endTime: Timestamp,
  ): Promise<Checkpoint[]> {
    return this.query({
      executionId,
      timestampRange: { start: startTime, end: endTime },
    });
  }

  /**
   * Get checkpoints by workflow ID and time range
   * @param workflowId Workflow ID
   * @param startTime Start timestamp (ms)
   * @param endTime End timestamp (ms)
   * @returns Checkpoints in time range
   */
  async getWorkflowCheckpointsByTimeRange(
    workflowId: string,
    startTime: Timestamp,
    endTime: Timestamp,
  ): Promise<Checkpoint[]> {
    return this.query({
      workflowId,
      timestampRange: { start: startTime, end: endTime },
    });
  }

  /**
   * Get checkpoints by tags
   * @param executionId Execution ID
   * @param tags Tag array
   * @returns Checkpoints with matching tags
   */
  async getByTags(executionId: string, tags: string[]): Promise<Checkpoint[]> {
    return this.query({ executionId, tags });
  }

  /**
   * Get checkpoints by multiple IDs
   * @param ids Checkpoint ID list
   * @returns Checkpoints with matching IDs
   */
  async getByIds(ids: string[]): Promise<Checkpoint[]> {
    return this.query({ ids });
  }
}
