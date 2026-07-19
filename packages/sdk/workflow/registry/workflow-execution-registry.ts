/**
 * WorkflowExecutionRegistry - WorkflowExecutionEntity Registry
 * Responsible for the memory storage and basic querying of WorkflowExecutionEntity objects.
 * It does not handle state transitions, persistence, or serialization.
 *
 * This module only exports class definitions; no instances are exported.
 * Instances are managed uniformly through the SingletonRegistry.
 */

import { WorkflowExecutionEntity } from "../entities/workflow-execution-entity.js";
import type { WorkflowExecutionStatus } from "@wf-agent/types";
import type { WorkflowExecutionStorageAdapter } from "@wf-agent/storage";
import type { WorkflowExecutionStorageMetadata } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import type { WorkflowStateCoordinator } from "../state-managers/workflow-state-coordinator.js";
import {
  ExecutionStore,
  CoordinatorStore,
} from "../../shared/registry/execution-registry-base.js";

const logger = createContextualLogger({ component: "WorkflowExecutionRegistry" });

/**
 * WorkflowExecutionRegistry - Workflow Execution Entity Registry
 *
 * Core Responsibilities:
 * - Manage active WorkflowExecutionEntity instances.
 * - Manage WorkflowStateCoordinator instances (for message access).
 * - Provide registration, query and deletion of instances.
 * - Support filtering by status.
 * - Support resource cleanup.
 *
 * Design Principles:
 * - Composes ExecutionStore for entities and CoordinatorStore for coordinators.
 * - Simple memory storage.
 * - Workflow-execution-safe (Map operations).
 * - Support for cleaning up expired instances.
 */
export class WorkflowExecutionRegistry {
  /** Entity storage */
  private store = new ExecutionStore<WorkflowExecutionEntity>();
  /** State coordinator storage */
  private coordinatorStore = new CoordinatorStore<WorkflowStateCoordinator>();
  /** Storage adapter (optional) */
  private storageAdapter?: WorkflowExecutionStorageAdapter;

  /**
   * Constructor
   * @param options Configuration options
   */
  constructor(options?: { storageAdapter?: WorkflowExecutionStorageAdapter }) {
    this.storageAdapter = options?.storageAdapter;
  }

  /**
   * Register WorkflowExecutionEntity
   * @param entity Workflow execution entity
   */
  register(entity: WorkflowExecutionEntity): void {
    this.store.register(entity);
    this.persistToStorage(entity).catch(() => {
      // Persistence errors are handled internally
    });
  }

  /**
   * Register WorkflowStateCoordinator
   * @param executionId Execution ID
   * @param stateCoordinator WorkflowStateCoordinator instance
   */
  registerStateCoordinator(executionId: string, stateCoordinator: WorkflowStateCoordinator): void {
    this.coordinatorStore.register(executionId, stateCoordinator);
  }

  /**
   * Get WorkflowStateCoordinator
   * @param executionId Execution ID
   * @returns WorkflowStateCoordinator instance or null
   */
  getStateCoordinator(executionId: string): WorkflowStateCoordinator | null {
    return this.coordinatorStore.get(executionId);
  }

  /**
   * Get WorkflowExecutionEntity
   * @param workflowExecutionId: Workflow Execution ID
   * @returns: An instance of WorkflowExecutionEntity or null
   */
  get(executionId: string): WorkflowExecutionEntity | null {
    return this.store.get(executionId) || null;
  }

  /**
   * Check if an entity exists.
   */
  has(executionId: string): boolean {
    return this.store.has(executionId);
  }

  /**
   * Get all registered entities.
   */
  getAll(): WorkflowExecutionEntity[] {
    return this.store.getAll();
  }

  /**
   * Get all registered entity IDs.
   */
  getAllIds(): string[] {
    return this.store.getAllIds();
  }

  /**
   * Get the number of registered entities.
   */
  size(): number {
    return this.store.size();
  }

  /**
   * Delete WorkflowExecutionEntity
   * @param executionId Workflow Execution ID
   */
  delete(executionId: string): void {
    this.store.delete(executionId);
    this.coordinatorStore.delete(executionId);

    // Remove from storage (async, non-blocking)
    this.removeFromStorage(executionId);
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
      this.store.delete(entity.id);
    }
    return terminatedEntities.length;
  }

  /**
   * Clear all entities and coordinators.
   */
  clear(): void {
    this.store.clear();
    this.coordinatorStore.clear();
  }

  /**
   * AsyncDispose for using in `await using` syntax
   * Calls clear() to clean up all entities and coordinators
   */
  async [Symbol.asyncDispose](): Promise<void> {
    this.clear();
  }

  // ========== Hierarchy-Aware Methods ==========

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

  // ============================================================
  // Storage Persistence Methods
  // ============================================================

  /**
   * Persist workflow execution to storage (if adapter is available)
   * @param entity Workflow execution entity to persist
   */
  private async persistToStorage(entity: WorkflowExecutionEntity): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    try {
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
      const data = encoder.encode(JSON.stringify(executionData));

      const metadata: WorkflowExecutionStorageMetadata = {
        executionId: entity.id,
        workflowId: entity.getWorkflowId(),
        workflowVersion: entity.getWorkflowVersion(),
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

      await this.storageAdapter.save(entity.id, data, metadata);
    } catch (error) {
      logger.error("Failed to persist workflow execution to storage", {
        executionId: entity.id,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Remove workflow execution from storage (if adapter is available)
   * @param executionId Execution ID to remove
   */
  private async removeFromStorage(executionId: string): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    try {
      await this.storageAdapter.delete(executionId);
    } catch (error) {
      logger.error("Failed to remove workflow execution from storage", {
        executionId,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Initialize registry from storage (preload all executions)
   * Note: This is typically not used for executions as they are created dynamically.
   * This method is provided for completeness and potential future use cases.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping initialization from storage");
      return;
    }

    try {
      const executionIds = await this.storageAdapter.list();
      logger.info("Found workflow executions in storage", { count: executionIds.length });
    } catch (error) {
      logger.error("Failed to initialize workflow execution registry from storage", {
        error: getErrorMessage(error),
      });
    }
  }
}