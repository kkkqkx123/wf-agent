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

/**
 * WorkflowExecutionRegistry - Workflow Execution Entity Registry
 *
 * Core Responsibilities:
 * - Manage active WorkflowExecutionEntity instances.
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

  /**
   * Register WorkflowExecutionEntity
   * @param workflowExecutionEntity An instance of WorkflowExecutionEntity
   */
  register(workflowExecutionEntity: WorkflowExecutionEntity): void {
    this.workflowExecutionEntities.set(workflowExecutionEntity.id, workflowExecutionEntity);
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
   */
  clear(): void {
    this.workflowExecutionEntities.clear();
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
}
