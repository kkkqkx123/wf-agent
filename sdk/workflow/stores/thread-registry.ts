/**
 * ThreadRegistry - ThreadEntity Registry
 * Responsible for the memory storage and basic querying of ThreadEntity objects.
 * It does not handle state transitions, persistence, or serialization.
 *
 * This module only exports class definitions; no instances are exported.
 * Instances are managed uniformly through the SingletonRegistry.
 */

import { ThreadEntity } from "../entities/index.js";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

/**
 * ThreadRegistry - ThreadEntity Registry
 *
 * Core Responsibilities:
 * - Manage active ThreadEntity instances.
 * - Provide registration, query and deletion of instances.
 * - Support filtering by status.
 * - Support resource cleanup.
 *
 * Design Principles:
 * - Simple memory storage.
 * - Thread-safe (Map operations).
 * - Support for cleaning up expired instances.
 */
export class ThreadRegistry {
  private threadEntities: Map<string, ThreadEntity> = new Map();

  /**
   * Register ThreadEntity
   * @param threadEntity An instance of ThreadEntity
   */
  register(threadEntity: ThreadEntity): void {
    this.threadEntities.set(threadEntity.id, threadEntity);
  }

  /**
   * Get ThreadEntity
   * @param threadId: Thread ID
   * @returns: An instance of ThreadEntity or null
   */
  get(threadId: string): ThreadEntity | null {
    return this.threadEntities.get(threadId) || null;
  }

  /**
   * Delete ThreadEntity
   * @param threadId Thread ID
   */
  delete(threadId: string): void {
    this.threadEntities.delete(threadId);
  }

  /**
   * Get all ThreadEntities
   * @returns Array of ThreadEntity
   */
  getAll(): ThreadEntity[] {
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
   * Check if ThreadEntity exists
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
   * @returns Array of ThreadEntity with the specified status
   */
  getByStatus(status: WorkflowExecutionStatus): ThreadEntity[] {
    return this.getAll().filter(entity => entity.getStatus() === status);
  }

  /**
   * Get running ThreadEntities
   * @returns Array of running ThreadEntity
   */
  getRunning(): ThreadEntity[] {
    return this.getByStatus("RUNNING");
  }

  /**
   * Get paused ThreadEntities
   * @returns Array of paused ThreadEntity
   */
  getPaused(): ThreadEntity[] {
    return this.getByStatus("PAUSED");
  }

  /**
   * Get completed ThreadEntities
   * @returns Array of completed ThreadEntity
   */
  getCompleted(): ThreadEntity[] {
    return this.getByStatus("COMPLETED");
  }

  /**
   * Get failed ThreadEntities
   * @returns Array of failed ThreadEntity
   */
  getFailed(): ThreadEntity[] {
    return this.getByStatus("FAILED");
  }

  /**
   * Get cancelled ThreadEntities
   * @returns Array of cancelled ThreadEntity
   */
  getCancelled(): ThreadEntity[] {
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
