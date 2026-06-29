/**
 * WorkflowExecutionRegistry - Workflow Execution Entity Registry
 *
 * Manages active WorkflowExecutionEntity instances using unified persistence framework.
 *
 * Responsibilities:
 * - Manage active WorkflowExecutionEntity instances
 * - Manage WorkflowStateCoordinator instances
 * - Provide CRUD operations and queries
 * - Support persistence with BLOCKING/ASYNC strategies
 * - Track consistency between memory and storage
 *
 * This implementation uses BasePersistentRegistry for unified persistence support,
 * providing:
 * - Event notifications for all persistence operations
 * - Configurable persistence strategies (BLOCKING/ASYNC)
 * - Data consistency verification
 * - Automatic failure tracking
 */

import { WorkflowExecutionEntity } from "../entities/workflow-execution-entity.js";
import type { WorkflowExecutionStatus } from "@wf-agent/types";
import type { WorkflowExecutionStorageAdapter } from "@wf-agent/storage";
import type { WorkflowExecutionStorageMetadata } from "@wf-agent/types";
import type { WorkflowStateCoordinator } from "../state-managers/workflow-state-coordinator.js";
import { BasePersistentRegistry, type IdExtractor } from "../../shared/persistence/index.js";

/**
 * WorkflowExecutionRegistry - Workflow Execution Entity Registry
 *
 * Extends BasePersistentRegistry to inherit unified persistence capabilities.
 * Adds workflow-specific query methods and state coordinator management.
 */
export class WorkflowExecutionRegistry extends BasePersistentRegistry<WorkflowExecutionEntity> {
  /** State coordinator storage */
  private stateCoordinatorMap: Map<string, WorkflowStateCoordinator> = new Map();

  /**
   * Constructor
   * @param options Configuration options
   */
  constructor(options?: {
    storageAdapter?: WorkflowExecutionStorageAdapter;
    persistenceConfig?: any;
  }) {
    super({
      storageAdapter: options?.storageAdapter,
      persistenceConfig: options?.persistenceConfig,
      registryName: "WorkflowExecutionRegistry",
    });
  }

  /**
   * Register WorkflowStateCoordinator
   * @param executionId Execution ID
   * @param stateCoordinator WorkflowStateCoordinator instance
   */
  registerStateCoordinator(executionId: string, stateCoordinator: WorkflowStateCoordinator): void {
    this.stateCoordinatorMap.set(executionId, stateCoordinator);
  }

  /**
   * Get WorkflowStateCoordinator
   * @param executionId Execution ID
   * @returns WorkflowStateCoordinator instance or null
   */
  getStateCoordinator(executionId: string): WorkflowStateCoordinator | null {
    return this.stateCoordinatorMap.get(executionId) || null;
  }

  /**
   * Delete WorkflowExecutionEntity and associated state coordinator
   * @param executionId Workflow Execution ID
   */
  delete(executionId: string): void {
    this.unregister(executionId);
    this.stateCoordinatorMap.delete(executionId);
  }

  /**
   * Check if the workflow is active.
   * @param workflowId: Workflow ID
   * @returns: Whether the workflow is active
   */
  isWorkflowActive(workflowId: string): boolean {
    return this.getAll().some(
      workflowExecutionEntity => workflowExecutionEntity.getWorkflowId() === workflowId,
    );
  }

  /**
   * Get WorkflowExecutionEntities by status
   * @param status: Execution status
   * @returns: Array of WorkflowExecutionEntity
   */
  getByStatus(status: WorkflowExecutionStatus): WorkflowExecutionEntity[] {
    return this.getAll().filter(
      workflowExecutionEntity => workflowExecutionEntity.getStatus() === status,
    );
  }

  /**
   * Get active WorkflowExecutionEntities (RUNNING or PAUSED)
   * @returns: Array of WorkflowExecutionEntity
   */
  getActive(): WorkflowExecutionEntity[] {
    return this.getAll().filter(workflowExecutionEntity => {
      const status = workflowExecutionEntity.getStatus();
      return status === "RUNNING" || status === "PAUSED";
    });
  }

  /**
   * Get WorkflowExecutionEntities by workflow ID
   * @param workflowId: Workflow ID
   * @returns: Array of WorkflowExecutionEntity
   */
  getByWorkflowId(workflowId: string): WorkflowExecutionEntity[] {
    return this.getAll().filter(
      workflowExecutionEntity => workflowExecutionEntity.getWorkflowId() === workflowId,
    );
  }

  /**
   * Get completed WorkflowExecutionEntities
   * @returns Array of WorkflowExecutionEntity
   */
  getCompleted(): WorkflowExecutionEntity[] {
    return this.getByStatus("COMPLETED");
  }

  /**
   * Get failed WorkflowExecutionEntities
   * @returns Array of WorkflowExecutionEntity
   */
  getFailed(): WorkflowExecutionEntity[] {
    return this.getByStatus("FAILED");
  }

  /**
   * Get cancelled WorkflowExecutionEntities
   * @returns Array of WorkflowExecutionEntity
   */
  getCancelled(): WorkflowExecutionEntity[] {
    return this.getByStatus("CANCELLED");
  }

  /**
   * Cleanup terminated (completed + failed + cancelled) instances
   * Calls cleanup on each terminated entity before removing from registry.
   * @returns Number of instances cleaned up
   */
  cleanupTerminated(): number {
    const terminatedEntities = [
      ...this.getCompleted(),
      ...this.getFailed(),
      ...this.getCancelled(),
    ];
    for (const entity of terminatedEntities) {
      entity.cleanup();
      this.unregister(entity.id);
    }
    return terminatedEntities.length;
  }

  /**
   * Query workflow executions with optional filters
   * @param filter Optional filter criteria
   * @returns Array of matching executions
   */
  query(filter?: {
    status?: WorkflowExecutionStatus;
    workflowId?: string;
  }): WorkflowExecutionEntity[] {
    let results = this.getAll();

    if (filter?.status) {
      results = results.filter(entity => entity.getStatus() === filter.status);
    }

    if (filter?.workflowId) {
      results = results.filter(entity => entity.getWorkflowId() === filter.workflowId);
    }

    return results;
  }

  /**
   * Get child WorkflowExecutionEntities by parent execution ID
   * Supports Workflow → Workflow hierarchy relationships
   *
   * @param parentExecutionId Parent execution ID (can be Workflow or Agent)
   * @returns Array of child WorkflowExecutionEntity instances
   */
  getChildrenByParentExecutionId(parentExecutionId: string): WorkflowExecutionEntity[] {
    return this.getAll().filter(entity => {
      const parentContext = entity.getParentContext();
      return parentContext?.parentId === parentExecutionId;
    });
  }

  /**
   * Get child WorkflowExecutionEntity IDs by parent execution ID
   *
   * @param parentExecutionId Parent execution ID
   * @returns Array of child WorkflowExecutionEntity IDs
   */
  getChildIdsByParentExecutionId(parentExecutionId: string): string[] {
    return this.getChildrenByParentExecutionId(parentExecutionId).map(entity => entity.id);
  }

  /**
   * Clear all WorkflowExecutionEntities
   * Calls the cleanup method of each entity before clearing.
   */
  override clear(): void {
    for (const entity of this.getAll()) {
      if (typeof entity.cleanup === "function") {
        entity.cleanup();
      }
    }
    for (const stateCoordinator of this.stateCoordinatorMap.values()) {
      stateCoordinator.cleanup();
    }
    super.clear();
    this.stateCoordinatorMap.clear();
  }

  /**
   * Enable await using pattern support
   * Delegates to clear() for resource release
   */
  async [Symbol.asyncDispose](): Promise<void> {
    this.clear();
  }

  // ============================================================
  // Protected methods - implement abstract methods
  // ============================================================

  /**
   * Get ID extractor for workflow execution entities
   * @protected
   */
  protected getIdExtractor(): IdExtractor<WorkflowExecutionEntity> {
    return {
      extractId: (entity: WorkflowExecutionEntity) => entity.id,
    };
  }

  /**
   * Serialize workflow execution entity to bytes
   * @protected
   */
  protected async serializeEntity(entity: WorkflowExecutionEntity): Promise<Uint8Array> {
    const encoder = new TextEncoder();
    const executionData = {
      id: entity.id,
      workflowId: entity.getWorkflowId(),
      status: entity.getStatus(),
      executionType: entity.getExecutionType(),
      currentNodeId: entity.getCurrentNodeId(),
      input: entity.getInput(),
      output: entity.getOutput(),
      startTime: entity.state.startTime,
      endTime: entity.state.endTime,
      error: entity.state.error,
      hierarchy: entity.getHierarchyMetadata(),
    };
    return encoder.encode(JSON.stringify(executionData));
  }

  /**
   * Build metadata for workflow execution entity
   * @protected
   */
  protected async buildMetadata(entity: WorkflowExecutionEntity): Promise<WorkflowExecutionStorageMetadata> {
    return {
      executionId: entity.id,
      workflowId: entity.getWorkflowId(),
      workflowVersion: "1.0", // TODO: Get from workflow definition
      status: entity.getStatus(),
      executionType: entity.getExecutionType(),
      currentNodeId: entity.getCurrentNodeId(),
      parentExecutionId: entity.getParentContext()?.parentId,
      startTime: entity.state.startTime || Date.now(),
      endTime: entity.state.endTime || undefined,
      tags: [],
      customFields: {
        nodeResultsCount: entity.getNodeResults().length,
      },
    };
  }

  /**
   * Get registry name for logging and diagnostics
   * @protected
   */
  protected getRegistryName(): string {
    return "WorkflowExecutionRegistry";
  }
}
