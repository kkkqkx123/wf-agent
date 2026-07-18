/**
 * AgentLoopRegistry - Agent Loop Registry
 *
 * Manages all active AgentLoopEntity instances.
 * Uses ExecutionStore for entity storage and CoordinatorStore for state coordinator storage.
 */

import type { ID } from "@wf-agent/types";
import type { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import { AgentLoopStatus } from "@wf-agent/types";
import type { AgentLoopStorageAdapter } from "@wf-agent/storage";
import type { AgentEntityMetadata } from "@wf-agent/types";
import type { IAgentExecutionRegistry, AgentExecutionFilter } from "./agent-execution-registry.js";
import type { AgentStateCoordinator } from "../state-managers/agent-state-coordinator.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import { generateId } from "../../utils/index.js";
import type { TaskStatus } from "@wf-agent/types";
import {
  ExecutionStore,
  CoordinatorStore,
} from "../../shared/registry/execution-registry-base.js";

const logger = createContextualLogger({ component: "AgentLoopRegistry" });

/**
 * Task Manager Interface for Agent Task Routing
 */
export interface AgentTaskManager {
  cancelTask(taskId: string): Promise<boolean>;
  getTaskStatus(taskId: string): AgentTaskInfo | null;
}

/**
 * Agent Task Info - Tracks task lifecycle (for async execution)
 */
export interface AgentTaskInfo {
  id: string;
  agentLoopId: ID;
  status: TaskStatus;
  submitTime: number;
  startTime?: number;
  completeTime?: number;
  result?: unknown;
  error?: Error;
  timeout?: number;
}

/**
 * Agent Task Statistics
 */
export interface AgentTaskStats {
  total: number;
  queued: number;
  running: number;
  completed: number;
  failed: number;
  cancelled: number;
  timeout: number;
}

/**
 * AgentLoopRegistry - Agent Loop Registry
 *
 * Core Responsibilities:
 * - Manage active AgentLoopEntity instances.
 * - Provide registration, query and deletion of instances
 * - Store AgentStateCoordinator alongside entities
 *
 * Design Principles:
 * - Composes ExecutionStore for entities and CoordinatorStore for coordinators
 * - Singleton model (managed via DI container)
 * - Workflow-execution-safe (Map operations)
 * - Support for cleaning up expired instances
 */
export class AgentLoopRegistry implements IAgentExecutionRegistry {
  /** Entity storage */
  private store = new ExecutionStore<AgentLoopEntity>();
  /** State coordinator storage */
  private coordinatorStore = new CoordinatorStore<AgentStateCoordinator>();
  /** Storage adapter (optional) */
  private storageAdapter?: AgentLoopStorageAdapter;

  /** Task Storage (for async execution) */
  private tasks: Map<string, AgentTaskInfo> = new Map();
  /** Task Manager Routing (for async execution) */
  private taskManagers: Map<string, AgentTaskManager> = new Map();
  /** Task Statistics */
  private taskStats: {
    completed: number;
    failed: number;
    cancelled: number;
    timeout: number;
  } = {
    completed: 0,
    failed: 0,
    cancelled: 0,
    timeout: 0,
  };

  /**
   * Constructor
   * @param options Configuration options
   */
  constructor(options?: { storageAdapter?: AgentLoopStorageAdapter }) {
    this.storageAdapter = options?.storageAdapter;
  }

  /**
   * Register AgentLoopEntity
   * @param entity The Agent Loop entity
   */
  register(entity: AgentLoopEntity): void {
    this.store.register(entity);
    this.persistToStorage(entity).catch(() => {
      // Persistence errors are handled internally
    });
  }

  /**
   * Logout AgentLoopEntity
   * @param id Instance ID
   * @returns Whether the logout was successful
   */
  unregister(id: ID): boolean {
    const entity = this.store.get(id);
    if (!entity) return false;

    this.store.delete(id);
    this.coordinatorStore.delete(id);
    this.removeFromStorage(id);

    return true;
  }

  /**
   * Register AgentStateCoordinator
   * @param agentLoopId Agent Loop ID
   * @param stateCoordinator AgentStateCoordinator instance
   */
  registerStateCoordinator(agentLoopId: ID, stateCoordinator: AgentStateCoordinator): void {
    this.coordinatorStore.register(agentLoopId, stateCoordinator);
  }

  /**
   * Get AgentStateCoordinator
   * @param agentLoopId Agent Loop ID
   * @returns AgentStateCoordinator instance or null
   */
  getStateCoordinator(agentLoopId: ID): AgentStateCoordinator | null {
    return this.coordinatorStore.get(agentLoopId);
  }

  /**
   * Get AgentLoopEntity
   * Tries to get from memory first, then attempts to load from storage if not found.
   * @param id instance ID
   * @returns Agent Loop entity, or undefined if it doesn't exist.
   */
  async get(id: ID): Promise<AgentLoopEntity | undefined> {
    // Try memory first
    const cached = this.store.get(id);
    if (cached) {
      return cached;
    }

    // If not in memory and storage adapter is available, try loading from storage
    if (this.storageAdapter) {
      logger.debug("Agent loop not in memory, attempting to load from storage", {
        agentLoopId: id,
      });
      // Note: Full restoration requires checkpoint mechanism via AgentLoopFactory.fromCheckpoint()
      // This method only provides basic data loading. For complete entity restoration,
      // use the checkpoint restore API instead.
      const loadedData = await this._loadFromStorage(id);
      if (loadedData) {
        logger.warn(
          "Loaded raw data from storage but cannot reconstruct full entity without checkpoint. " +
            "Use AgentLoopFactory.fromCheckpoint() for complete restoration.",
          { agentLoopId: id },
        );
      }
    }

    return undefined;
  }

  /**
   * Check if an entity exists.
   */
  has(id: ID): boolean {
    return this.store.has(id);
  }

  /**
   * Get all active instances
   */
  getAll(): AgentLoopEntity[] {
    return this.store.getAll();
  }

  /**
   * Get all instance IDs
   */
  getAllIds(): ID[] {
    return this.store.getAllIds();
  }

  /**
   * Get the number of registered entities.
   */
  size(): number {
    return this.store.size();
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
   * Query agent loop entities with optional filter
   *
   * Supports filtering by:
   * - status: Filter by execution status
   * - parentWorkflowId: Filter by parent workflow execution ID
   *
   * @param filter Optional filter criteria
   * @returns Filtered array of AgentLoopEntity instances
   */
  query(filter?: AgentExecutionFilter): AgentLoopEntity[] {
    let results = this.getAll();

    if (filter?.status) {
      results = results.filter(entity => entity.getStatus() === filter.status);
    }

    if (filter?.parentWorkflowId) {
      results = results.filter(entity => {
        const parent = entity.getParentContext();
        return parent?.parentType === "WORKFLOW" && parent.parentId === filter.parentWorkflowId;
      });
    }

    return results;
  }

  /**
   * Cleaning up terminated (completed + failed + cancelled) instances
   * Calls cleanup on each terminated entity before unregistering.
   * @returns Number of instances cleaned up
   */
  cleanupTerminated(): number {
    const terminatedEntities = [
      ...this.getCompleted(),
      ...this.getFailed(),
      ...this.getByStatus(AgentLoopStatus.CANCELLED),
    ];
    for (const entity of terminatedEntities) {
      entity.cleanup();
      this.unregister(entity.id);
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

  // ============================================================
  // Storage Persistence Methods
  // ============================================================

  /**
   * Persist agent loop to storage (if adapter is available)
   * @param entity Agent loop entity to persist
   */
  private async persistToStorage(entity: AgentLoopEntity): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    try {
      const encoder = new TextEncoder();
      const agentData = {
        id: entity.id,
        status: entity.getStatus(),
        currentIteration: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        startTime: entity.state.startTime,
        endTime: entity.state.endTime,
        maxIterations: entity.config?.maxIterations,
      };
      const data = encoder.encode(JSON.stringify(agentData));

      const metadata: AgentEntityMetadata = {
        agentLoopId: entity.id,
        status: entity.getStatus(),
        createdAt: entity.state.startTime || Date.now(),
        updatedAt: entity.state.endTime || undefined,
        completedAt: entity.state.endTime || undefined,
        tags: [],
        customFields: {
          currentIteration: entity.state.currentIteration,
          toolCallCount: entity.state.toolCallCount,
          maxIterations: entity.config?.maxIterations,
        },
      };

      await this.storageAdapter.save(entity.id, data, metadata);
    } catch (error) {
      logger.error("Failed to persist agent loop to storage", {
        agentLoopId: entity.id,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Remove agent loop from storage (if adapter is available)
   * @param agentLoopId Agent loop ID to remove
   */
  private async removeFromStorage(agentLoopId: string): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    try {
      await this.storageAdapter.delete(agentLoopId);
    } catch (error) {
      logger.error("Failed to remove agent loop from storage", {
        agentLoopId,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Load agent loop data from storage (if adapter is available)
   *
   * IMPORTANT: This method loads raw serialized data, NOT a fully reconstructed AgentLoopEntity.
   * To restore a complete entity with all dependencies (config, conversationManager, etc.),
   * use AgentLoopFactory.fromCheckpoint() which properly handles:
   * - Loading checkpoint from storage
   * - Extracting state snapshot
   * - Reconstructing entity with re-provided config
   * - Restoring conversation manager and other runtime components
   *
   * @param agentLoopId Agent loop ID to load
   * @returns Raw agent loop data or null (cannot be used directly as AgentLoopEntity)
   */
  private async _loadFromStorage(agentLoopId: string): Promise<unknown | null> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, cannot load from storage");
      return null;
    }

    try {
      const data = await this.storageAdapter.load(agentLoopId);
      if (!data) {
        logger.debug("No data found in storage for agent loop", { agentLoopId });
        return null;
      }

      // Deserialize agent loop data
      const decoder = new TextDecoder();
      const agentData = JSON.parse(decoder.decode(data));

      logger.debug("Raw agent loop data loaded from storage", { agentLoopId });
      return agentData;
    } catch (error) {
      logger.error("Failed to load agent loop data from storage", {
        agentLoopId,
        error: getErrorMessage(error),
      });
      return null;
    }
  }

  /**
   * Initialize registry from storage (preload all agent loops)
   * Note: This is typically not used for agent loops as they are created dynamically.
   * This method is provided for completeness and potential future use cases.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping initialization from storage");
      return;
    }

    try {
      const agentLoopIds = await this.storageAdapter.list();
      logger.info("Found agent loops in storage", { count: agentLoopIds.length });
    } catch (error) {
      logger.error("Failed to initialize agent loop registry from storage", {
        error: getErrorMessage(error),
      });
    }
  }

  // ============================================================
  // Task Management Methods (for async execution)
  // ============================================================

  /**
   * Register an agent loop as a task (for async execution)
   * @param entity Agent loop entity
   * @param manager Task manager for this task
   * @param timeout Optional timeout in milliseconds
   * @returns Task ID
   */
  registerAsTask(entity: AgentLoopEntity, manager: AgentTaskManager, timeout?: number): string {
    const taskId = generateId();

    const taskInfo: AgentTaskInfo = {
      id: taskId,
      agentLoopId: entity.id,
      status: "QUEUED",
      submitTime: now(),
      timeout,
    };

    this.tasks.set(taskId, taskInfo);
    this.taskManagers.set(taskId, manager);

    logger.debug("Agent loop registered as task", { taskId, agentLoopId: entity.id });

    return taskId;
  }

  /**
   * Update task status to RUNNING
   * @param taskId Task ID
   */
  updateTaskStatusToRunning(taskId: string): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "RUNNING";
      taskInfo.startTime = now();
      logger.debug("Task status updated to RUNNING", { taskId });
    }
  }

  /**
   * Update task status to COMPLETED
   * @param taskId Task ID
   * @param result Execution result
   */
  updateTaskStatusToCompleted(taskId: string, result?: unknown): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "COMPLETED";
      taskInfo.completeTime = now();
      taskInfo.result = result;
      this.taskStats.completed++;
      logger.debug("Task status updated to COMPLETED", { taskId });
    }
  }

  /**
   * Update task status to FAILED
   * @param taskId Task ID
   * @param error Error details
   */
  updateTaskStatusToFailed(taskId: string, error: Error): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "FAILED";
      taskInfo.completeTime = now();
      taskInfo.error = error;
      this.taskStats.failed++;
      logger.debug("Task status updated to FAILED", { taskId });
    }
  }

  /**
   * Update task status to CANCELLED
   * @param taskId Task ID
   */
  updateTaskStatusToCancelled(taskId: string): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "CANCELLED";
      taskInfo.completeTime = now();
      this.taskStats.cancelled++;
      logger.debug("Task status updated to CANCELLED", { taskId });
    }
  }

  /**
   * Get task information
   * @param taskId Task ID
   * @returns Task information or null
   */
  getTaskInfo(taskId: string): AgentTaskInfo | null {
    return this.tasks.get(taskId) || null;
  }

  /**
   * Get tasks by status
   * @param status Task status
   * @returns Array of task IDs
   */
  getTasksByStatus(status: TaskStatus): string[] {
    return Array.from(this.tasks.entries())
      .filter(([_, task]) => task.status === status)
      .map(([taskId]) => taskId);
  }

  /**
   * Get task statistics
   * @returns Task statistics
   */
  getTaskStats(): AgentTaskStats {
    const tasks = Array.from(this.tasks.values());
    return {
      total: tasks.length,
      queued: tasks.filter(t => t.status === "QUEUED").length,
      running: tasks.filter(t => t.status === "RUNNING").length,
      completed: this.taskStats.completed,
      failed: this.taskStats.failed,
      cancelled: this.taskStats.cancelled,
      timeout: this.taskStats.timeout,
    };
  }

  /**
   * Cancel a task
   * @param taskId Task ID
   * @returns Whether cancellation was successful
   */
  async cancelTask(taskId: string): Promise<boolean> {
    const manager = this.taskManagers.get(taskId);
    if (!manager) {
      return false;
    }

    const success = await manager.cancelTask(taskId);
    if (success) {
      this.updateTaskStatusToCancelled(taskId);
      this.taskManagers.delete(taskId);
    }

    return success;
  }

  /**
   * Delete a task
   * @param taskId Task ID
   * @returns Whether deletion was successful
   */
  async deleteTask(taskId: string): Promise<boolean> {
    this.taskManagers.delete(taskId);
    return this.tasks.delete(taskId);
  }

  /**
   * Cleanup expired tasks (for async execution)
   * @param retentionTime Retention time in milliseconds
   * @returns Number of tasks cleaned up
   */
  async cleanupExpiredTasks(retentionTime: number = 60 * 60 * 1000): Promise<number> {
    const currentTime = now();
    let cleanedCount = 0;

    for (const [taskId, taskInfo] of this.tasks.entries()) {
      if (
        (taskInfo.status === "COMPLETED" ||
          taskInfo.status === "FAILED" ||
          taskInfo.status === "CANCELLED" ||
          taskInfo.status === "TIMEOUT") &&
        taskInfo.completeTime &&
        currentTime - taskInfo.completeTime > retentionTime
      ) {
        this.tasks.delete(taskId);
        this.taskManagers.delete(taskId);
        cleanedCount++;
      }
    }

    if (cleanedCount > 0) {
      logger.debug("Cleaned up expired tasks", { cleanedCount });
    }

    return cleanedCount;
  }

  /**
   * Find task ID by agent loop ID
   * @param agentLoopId Agent loop ID
   * @returns Task ID or null
   */
  findTaskIdByAgentLoopId(agentLoopId: ID): string | null {
    for (const [taskId, taskInfo] of this.tasks.entries()) {
      if (taskInfo.agentLoopId === agentLoopId) {
        return taskId;
      }
    }
    return null;
  }
}