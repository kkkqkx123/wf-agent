/**
 * WorkflowExecutionRegistry - WorkflowExecutionEntity Registry
 * Responsible for the memory storage and basic querying of WorkflowExecutionEntity objects.
 * It does not handle state transitions, persistence, or serialization.
 *
 * This module only exports class definitions; no instances are exported.
 * Instances are managed uniformly through the SingletonRegistry.
 */

import { WorkflowExecutionEntity } from "../entities/index.js";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

/**
 * WorkflowExecutionRegistry - WorkflowExecutionEntity Registry
 *
 * Core Responsibilities:
 * - Manage active WorkflowExecutionEntity instances.
 * - Provide registration, query and deletion of instances.
 * - Support filtering by status.
 * - Support resource cleanup.
 *
 * Design Principles:
 * - Simple memory storage.
 * - Thread-safe (Map operations).
 * - Support for cleaning up expired instances.
 */
export class WorkflowExecutionRegistry {
  private threadEntities: Map<string, WorkflowExecutionEntity> = new Map();

  /**
   * Register WorkflowExecutionEntity
   * @param threadEntity An instance of WorkflowExecutionEntity
   */
  register(threadEntity: WorkflowExecutionEntity): void {
    this.threadEntities.set(threadEntity.id, threadEntity);
  }

  /**
   * Get WorkflowExecutionEntity
   * @param threadId: Thread ID
   * @returns: An instance of WorkflowExecutionEntity or null
   */
  get(threadId: string): WorkflowExecutionEntity | null {
    return this.threadEntities.get(threadId) || null;
  }

  /**
   * Delete WorkflowExecutionEntity
   * @param threadId Thread ID
   */
  delete(threadId: string): void {
    this.threadEntities.delete(threadId);
  }

  /**
   * Get all ThreadEntities
   * @returns Array of WorkflowExecutionEntity
   */
  getAll(): WorkflowExecutionEntity[] {
    return Array.from(this.threadEntities.values());
  }

  /**
   * Get all thread IDs
   * @returns Array of thread IDs
   */
  getAllIds(): string[] {
    return Array.from(this.threadEntities.keys());
  }

  /**
   * Get the number of instances
   */
  size(): number {
    return this.threadEntities.size;
  }

  /**
   * Clear all ThreadEntities
   */
  clear(): void {
    this.threadEntities.clear();
  }

  /**
   * Check if WorkflowExecutionEntity exists
   * @param threadId: Thread ID
   * @returns: Whether it exists or not
   */
  has(threadId: string): boolean {
    return this.threadEntities.has(threadId);
  }

  /**
   * Check if the workflow is active.
   * @param workflowId: Workflow ID
   * @returns: Whether the workflow is active
   */
  isWorkflowActive(workflowId: string): boolean {
    return this.getAll().some(threadEntity => threadEntity.getWorkflowId() === workflowId);
  }

  // ========== Status-based Query Methods ==========

  /**
   * Get ThreadEntities by status
   * @param status Thread status
   * @returns Array of WorkflowExecutionEntity with the specified status
   */
  getByStatus(status: WorkflowExecutionStatus): WorkflowExecutionEntity[] {
    return this.getAll().filter(entity => entity.getStatus() === status);
  }

  /**
   * Get running ThreadEntities
   * @returns Array of running WorkflowExecutionEntity
   */
  getRunning(): WorkflowExecutionEntity[] {
    return this.getByStatus("RUNNING");
  }

  /**
   * Get paused ThreadEntities
   * @returns Array of paused WorkflowExecutionEntity
   */
  getPaused(): WorkflowExecutionEntity[] {
    return this.getByStatus("PAUSED");
  }

  /**
   * Get completed ThreadEntities
   * @returns Array of completed WorkflowExecutionEntity
   */
  getCompleted(): WorkflowExecutionEntity[] {
    return this.getByStatus("COMPLETED");
  }

  /**
   * Get failed ThreadEntities
   * @returns Array of failed WorkflowExecutionEntity
   */
  getFailed(): WorkflowExecutionEntity[] {
    return this.getByStatus("FAILED");
  }

  /**
   * Get cancelled ThreadEntities
   * @returns Array of cancelled WorkflowExecutionEntity
   */
  getCancelled(): WorkflowExecutionEntity[] {
    return this.getByStatus("CANCELLED");
  }

  // ========== Resource Cleanup Methods ==========

  /**
   * Cleanup completed instances
   * Removes all completed ThreadEntities from the registry.
   * @returns Number of instances cleaned up
   */
  cleanupCompleted(): number {
    const completedIds = this.getCompleted().map(e => e.id);
    for (const id of completedIds) {
      this.delete(id);
    }
    return completedIds.length;
  }

  /**
   * Cleanup failed instances
   * Removes all failed ThreadEntities from the registry.
   * @returns Number of instances cleaned up
   */
  cleanupFailed(): number {
    const failedIds = this.getFailed().map(e => e.id);
    for (const id of failedIds) {
      this.delete(id);
    }
    return failedIds.length;
  }

  /**
   * Cleanup cancelled instances
   * Removes all cancelled ThreadEntities from the registry.
   * @returns Number of instances cleaned up
   */
  cleanupCancelled(): number {
    const cancelledIds = this.getCancelled().map(e => e.id);
    for (const id of cancelledIds) {
      this.delete(id);
    }
    return cancelledIds.length;
  }

  /**
   * Cleanup all resources
   * Calls the cleanup method of each entity before clearing the registry.
   */
  cleanup(): void {
    for (const entity of this.threadEntities.values()) {
      // Call cleanup method if available
      if (typeof entity.cleanup === "function") {
        entity.cleanup();
      }
    }
    this.threadEntities.clear();
  }
}
