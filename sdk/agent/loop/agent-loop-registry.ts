/**
 * AgentLoopRegistry - Agent Loop Registry
 *
 * Manages all active AgentLoopEntity instances.
 * Refer to the WorkflowExecutionRegistry design pattern.
 */

import type { ID } from "@wf-agent/types";
import type { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import { AgentLoopStatus } from "@wf-agent/types";

/**
 * AgentLoopRegistry - Agent Loop Registry
 *
 * Core Responsibilities:
 * - Manage active AgentLoopEntity instances.
 * - Provide registration, query and deletion of instances
 * - Provides registration, query, and deletion of instances.
 *
 * Design Principles:
 * - Singleton model (managed via DI container)
 * - Thread-safe (Map operations)
 * - Support for cleaning up expired instances
 */
export class AgentLoopRegistry {
  /** Instance Storage */
  private entities: Map<ID, AgentLoopEntity> = new Map();

  /**
   * Register AgentLoopEntity
   * @param entity The Agent Loop entity
   */
  register(entity: AgentLoopEntity): void {
    this.entities.set(entity.id, entity);
  }

  /**
   * Logout AgentLoopEntity
   * @param id Instance ID
   * @returns Whether the logout was successful
   */
  unregister(id: ID): boolean {
    return this.entities.delete(id);
  }

  /**
   * Get AgentLoopEntity
   * @param id instance ID
   * @returns Agent Loop entity, or undefined if it doesn't exist.
   */
  get(id: ID): AgentLoopEntity | undefined {
    return this.entities.get(id);
  }

  /**
   * Checking if an instance exists
   * @param id instance ID
   */
  has(id: ID): boolean {
    return this.entities.has(id);
  }

  /**
   * Get all active instances
   */
  getAll(): AgentLoopEntity[] {
    return Array.from(this.entities.values());
  }

  /**
   * Get all instance IDs
   */
  getAllIds(): ID[] {
    return Array.from(this.entities.keys());
  }

  /**
   * Get the number of instances
   */
  size(): number {
    return this.entities.size;
  }

  /**
   * Getting Examples by Status
   * @param status Execution status
   */
  getByStatus(status: AgentLoopStatus): AgentLoopEntity[] {
    return this.getAll().filter(entity => entity.getStatus() === status);
  }

  /**
   * Getting a running instance
   */
  getRunning(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.RUNNING);
  }

  /**
   * Getting a Suspended Instance
   */
  getPaused(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.PAUSED);
  }

  /**
   * Getting Completed Instances
   */
  getCompleted(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.COMPLETED);
  }

  /**
   * Getting failed instances
   */
  getFailed(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.FAILED);
  }

  /**
   * Cleaning up completed instances
   * @returns Number of instances cleaned up
   */
  cleanupCompleted(): number {
    const completedIds = this.getCompleted().map(e => e.id);
    for (const id of completedIds) {
      this.unregister(id);
    }
    return completedIds.length;
  }

  /**
   * Clear all instances
   */
  clear(): void {
    this.entities.clear();
  }

  /**
   * Cleaning up resources
   * Calls the cleanup method of each entity before cleaning it up
   */
  cleanup(): void {
    for (const entity of this.entities.values()) {
      if (typeof entity.cleanup === "function") {
        entity.cleanup();
      }
    }
    this.entities.clear();
  }

  // ========== Parent Thread Relation Methods ==========

  /**
   * Get AgentLoopEntities by parent thread ID
   * @param threadId Parent thread ID
   * @returns Array of AgentLoopEntity with the specified parent thread ID
   */
  getByParentThreadId(threadId: ID): AgentLoopEntity[] {
    return this.getAll().filter(entity => entity.parentThreadId === threadId);
  }

  /**
   * Get AgentLoopEntity IDs by parent thread ID
   * @param threadId Parent thread ID
   * @returns Array of AgentLoopEntity IDs
   */
  getIdsByParentThreadId(threadId: ID): ID[] {
    return this.getByParentThreadId(threadId).map(entity => entity.id);
  }

  /**
   * Cleanup AgentLoopEntities by parent thread ID
   * Stops and removes all AgentLoopEntities associated with the specified thread.
   * @param threadId Parent thread ID
   * @returns Number of instances cleaned up
   */
  cleanupByParentThreadId(threadId: ID): number {
    const entities = this.getByParentThreadId(threadId);
    for (const entity of entities) {
      // Stop the entity if it's running
      if (entity.isRunning()) {
        entity.stop();
      }
      // Cleanup resources
      if (typeof entity.cleanup === "function") {
        entity.cleanup();
      }
      this.unregister(entity.id);
    }
    return entities.length;
  }

  /**
   * Get running AgentLoopEntities by parent thread ID
   * @param threadId Parent thread ID
   * @returns Array of running AgentLoopEntity
   */
  getRunningByParentThreadId(threadId: ID): AgentLoopEntity[] {
    return this.getByParentThreadId(threadId).filter(entity => entity.isRunning());
  }

  /**
   * Get paused AgentLoopEntities by parent thread ID
   * @param threadId Parent thread ID
   * @returns Array of paused AgentLoopEntity
   */
  getPausedByParentThreadId(threadId: ID): AgentLoopEntity[] {
    return this.getByParentThreadId(threadId).filter(entity => entity.isPaused());
  }

  // ========== Parent Workflow Execution Relation Methods (Aliases) ==========

  /**
   * Get AgentLoopEntities by parent workflow execution ID
   * @param workflowExecutionId Parent workflow execution ID
   * @returns Array of AgentLoopEntity with the specified parent workflow execution ID
   */
  getByParentWorkflowExecutionId(workflowExecutionId: ID): AgentLoopEntity[] {
    return this.getByParentThreadId(workflowExecutionId);
  }

  /**
   * Get AgentLoopEntity IDs by parent workflow execution ID
   * @param workflowExecutionId Parent workflow execution ID
   * @returns Array of AgentLoopEntity IDs
   */
  getIdsByParentWorkflowExecutionId(workflowExecutionId: ID): ID[] {
    return this.getIdsByParentThreadId(workflowExecutionId);
  }

  /**
   * Cleanup AgentLoopEntities by parent workflow execution ID
   * Stops and removes all AgentLoopEntities associated with the specified workflow execution.
   * @param workflowExecutionId Parent workflow execution ID
   * @returns Number of instances cleaned up
   */
  cleanupByParentWorkflowExecutionId(workflowExecutionId: ID): number {
    return this.cleanupByParentThreadId(workflowExecutionId);
  }

  /**
   * Get running AgentLoopEntities by parent workflow execution ID
   * @param workflowExecutionId Parent workflow execution ID
   * @returns Array of running AgentLoopEntity
   */
  getRunningByParentWorkflowExecutionId(workflowExecutionId: ID): AgentLoopEntity[] {
    return this.getRunningByParentThreadId(workflowExecutionId);
  }

  /**
   * Get paused AgentLoopEntities by parent workflow execution ID
   * @param workflowExecutionId Parent workflow execution ID
   * @returns Array of paused AgentLoopEntity
   */
  getPausedByParentWorkflowExecutionId(workflowExecutionId: ID): AgentLoopEntity[] {
    return this.getPausedByParentThreadId(workflowExecutionId);
  }
}
