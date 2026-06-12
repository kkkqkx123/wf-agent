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
 * - Simple memory storage.
 * - Workflow-execution-safe (Map operations).
 * - Support for cleaning up expired instances.
 */
export class WorkflowExecutionRegistry {
  private workflowExecutionEntities: Map<string, WorkflowExecutionEntity> = new Map();
  private stateCoordinatorMap: Map<string, WorkflowStateCoordinator> = new Map();
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
   * @param workflowExecutionEntity An instance of WorkflowExecutionEntity
   */
  register(workflowExecutionEntity: WorkflowExecutionEntity): void {
    this.workflowExecutionEntities.set(workflowExecutionEntity.id, workflowExecutionEntity);
    
    // Persist to storage (async, non-blocking)
    this.persistToStorage(workflowExecutionEntity).catch(error => {
      logger.error("Failed to persist workflow execution to storage during register", {
        executionId: workflowExecutionEntity.id,
        error: getErrorMessage(error),
      });
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
   * Get WorkflowExecutionEntity
   * @param workflowExecutionId: Workflow Execution ID
   * @returns: An instance of WorkflowExecutionEntity or null
   */
  get(executionId: string): WorkflowExecutionEntity | null {
    return this.workflowExecutionEntities.get(executionId) || null;
  }

  /**
   * Delete WorkflowExecutionEntity
   * @param executionId Workflow Execution ID
   */
  delete(executionId: string): void {
    this.workflowExecutionEntities.delete(executionId);
    this.stateCoordinatorMap.delete(executionId);
    
    // Remove from storage (async, non-blocking)
    this.removeFromStorage(executionId).catch(error => {
      logger.error("Failed to remove workflow execution from storage during delete", {
        executionId,
        error: getErrorMessage(error),
      });
    });
  }

  /**
   * Get all WorkflowExecutionEntities
   * @returns Array of WorkflowExecutionEntity
   */
  getAll(): WorkflowExecutionEntity[] {
    return Array.from(this.workflowExecutionEntities.values());
  }

  /**
   * Get all execution IDs
   * @returns Array of execution IDs
   */
  getAllIds(): string[] {
    return Array.from(this.workflowExecutionEntities.keys());
  }

  /**
   * Get the number of instances
   */
  size(): number {
    return this.workflowExecutionEntities.size;
  }

  /**
   * Clear all WorkflowExecutionEntities
   * Calls the cleanup method of each entity before clearing.
   */
  clear(): void {
    for (const entity of this.workflowExecutionEntities.values()) {
      if (typeof entity.cleanup === "function") {
        entity.cleanup();
      }
    }
    for (const stateCoordinator of this.stateCoordinatorMap.values()) {
      stateCoordinator.cleanup();
    }
    this.workflowExecutionEntities.clear();
    this.stateCoordinatorMap.clear();
  }

  /**
   * Enable await using pattern support
   * Delegates to clear() for resource release
   */
  async [Symbol.asyncDispose](): Promise<void> {
    this.clear();
  }

  /**
   * Check if WorkflowExecutionEntity exists
   * @param workflowExecutionId: Workflow Execution ID
   * @returns: Whether it exists or not
   */
  has(executionId: string): boolean {
    return this.workflowExecutionEntities.has(executionId);
  }

  /**
   * Check if the workflow is active.
   * @param workflowId: Workflow ID
   * @returns: Whether the workflow is active
   */
  isWorkflowActive(workflowId: string): boolean {
    return this.getAll().some(workflowExecutionEntity => workflowExecutionEntity.getWorkflowId() === workflowId);
  }

  /**
   * Get WorkflowExecutionEntities by status
   * @param status: Execution status
   * @returns: Array of WorkflowExecutionEntity
   */
  getByStatus(status: WorkflowExecutionStatus): WorkflowExecutionEntity[] {
    return this.getAll().filter(workflowExecutionEntity => workflowExecutionEntity.getStatus() === status);
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
    return this.getAll().filter(workflowExecutionEntity => workflowExecutionEntity.getWorkflowId() === workflowId);
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
      this.workflowExecutionEntities.delete(entity.id);
    }
    return terminatedEntities.length;
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
      logger.debug("No storage adapter configured, skipping workflow execution persistence");
      return;
    }

    try {
      // Serialize execution state to bytes
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

      // Create metadata matching WorkflowExecutionStorageMetadata interface
      const metadata: WorkflowExecutionStorageMetadata = {
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

      await this.storageAdapter.save(entity.id, data, metadata);
      logger.debug("Workflow execution persisted to storage", {
        executionId: entity.id,
        status: entity.getStatus(),
      });
    } catch (error) {
      // Log error but don't throw - storage failure should not affect core functionality
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
      logger.debug("No storage adapter configured, skipping workflow execution removal from storage");
      return;
    }

    try {
      await this.storageAdapter.delete(executionId);
      logger.debug("Workflow execution removed from storage", { executionId });
    } catch (error) {
      // Log error but don't throw - storage failure should not affect core functionality
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
      
      // Note: We don't automatically load all executions into memory
      // Executions are typically loaded on-demand when needed
      // This method can be extended to implement caching strategies if needed
    } catch (error) {
      logger.error("Failed to initialize workflow execution registry from storage", {
        error: getErrorMessage(error),
      });
    }
  }
}
