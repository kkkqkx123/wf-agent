/**
 * TaskRegistry - Task Registry (Global Singleton Service)
 *
 * Responsibilities:
 * - Stores and manages information about all tasks
 * - Tracks task status, execution results, timestamps, etc.
 * - Provides functionality for querying and cleaning up tasks
 * - Routes task operations to the appropriate managers
 * - Supports optional persistence through TaskStorageCallback
 *
 * Design Principles:
 * - Global singleton, accessible through SingletonRegistry
 * - Thread-safe management of task information
 * - Supports regular cleaning of expired tasks
 * - Provides manager routing functionality
 * - Optional persistence layer for task data
 */

import { generateId } from "../../../utils/index.js";
import { now } from "@wf-agent/common-utils";
import type { ThreadEntity } from "../../entities/index.js";
import type { WorkflowExecutionResult, TaskStorageMetadata } from "@wf-agent/types";
import {
  TaskStatus,
  type TaskInfo,
  type StoredTaskInfo,
  type ExecutionInstance,
  type ExecutionInstanceType,
  isThreadInstance,
  hasLoadedInstance,
  isStoredTaskInfo,
} from "../../../core/types/index.js";
import type { TaskStorageCallback } from "@wf-agent/storage";
import {
  SerializationRegistry,
  ErrorSerializer,
  type TaskSnapshot,
  TaskSerializationUtils,
  registerTaskSerializer,
} from "../../../core/serialization/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TaskRegistry" });

registerTaskSerializer();

/**
 * Task Manager Interface
 * All managers that need to manage tasks must implement this interface.
 */
export interface TaskManager {
  /**
   * Cancel Task
   * @param taskId Task ID
   * @returns Whether the cancellation was successful
   */
  cancelTask(taskId: string): Promise<boolean>;

  /**
   * Get task status
   * @param taskId Task ID
   * @returns Task information
   */
  getTaskStatus(taskId: string): TaskInfo | null;
}

/**
 * TaskRegistry Configuration
 */
export interface TaskRegistryConfig {
  /** Optional storage callback for persistence */
  storageCallback?: TaskStorageCallback;
  /** Enable auto-persist on status changes */
  autoPersist?: boolean;
}

/**
 * TaskRegistry - Task Registry (global singleton)
 *
 * Supports both in-memory and persistent storage modes:
 * - Without storageCallback: Pure in-memory mode (default, backward compatible)
 * - With storageCallback: Persistent mode with automatic sync
 */
export class TaskRegistry {
  private static instance: TaskRegistry;

  /**
   * Task Mapping
   * Stores both active (with loaded instance) and stored (with reference) tasks
   */
  private tasks: Map<string, TaskInfo | StoredTaskInfo> = new Map();

  /**
   * Task ID to Manager Mapping
   */
  private taskManagers: Map<string, TaskManager> = new Map();

  /**
   * Statistics counter
   */
  private stats = {
    completed: 0,
    failed: 0,
    cancelled: 0,
    timeout: 0,
  };

  /**
   * Storage callback for persistence (optional)
   */
  private storageCallback?: TaskStorageCallback;

  /**
   * Auto-persist flag
   */
  private autoPersist: boolean = false;

  /**
   * Initialization flag
   */
  private initialized: boolean = false;

  private constructor() {}

  /**
   * Obtain a singleton instance
   */
  static getInstance(): TaskRegistry {
    if (!TaskRegistry.instance) {
      TaskRegistry.instance = new TaskRegistry();
    }
    return TaskRegistry.instance;
  }

  /**
   * Initialize the registry with optional persistence
   * @param config Configuration options
   */
  async initialize(config?: TaskRegistryConfig): Promise<void> {
    if (config?.storageCallback) {
      this.storageCallback = config.storageCallback;
      this.autoPersist = config.autoPersist ?? true;

      // Initialize storage if not already initialized
      try {
        await this.storageCallback.initialize();

        // Load existing tasks from storage
        await this.loadFromStorage();

        this.initialized = true;
      } catch (error) {
        // If storage initialization fails, fall back to in-memory mode
        logger.warn("Failed to initialize task storage, falling back to in-memory mode", { error });
        this.storageCallback = undefined;
        this.autoPersist = false;
      }
    }
  }

  /**
   * Load tasks from storage
   */
  private async loadFromStorage(): Promise<void> {
    if (!this.storageCallback) return;

    try {
      const taskIds = await this.storageCallback.list();
      const registry = SerializationRegistry.getInstance();

      for (const taskId of taskIds) {
        const data = await this.storageCallback.load(taskId);
        if (data) {
          const snapshot = registry.deserialize<TaskSnapshot>("task", data);
          const storedTask: StoredTaskInfo = {
            id: snapshot.id,
            instanceType: snapshot.instanceType,
            instanceRef: { type: "reference", instanceId: snapshot.instanceId },
            status: snapshot.status,
            submitTime: snapshot.submitTime,
            startTime: snapshot.startTime,
            completeTime: snapshot.completeTime,
            timeout: snapshot.timeout,
          };

          if (snapshot.result) {
            storedTask.result = TaskSerializationUtils.deserializeThreadResult(snapshot.result);
          }

          if (snapshot.error) {
            storedTask.error = ErrorSerializer.deserialize(snapshot.error);
          }

          this.tasks.set(taskId, storedTask);
        }
      }
    } catch (error) {
      logger.warn("Failed to load tasks from storage", { error });
    }
  }

  /**
   * Persist a single task to storage
   * @param taskId Task ID
   */
  private async persistTask(taskId: string): Promise<void> {
    if (!this.storageCallback || !this.autoPersist) return;

    const taskInfo = this.tasks.get(taskId);
    if (!taskInfo) return;

    try {
      const registry = SerializationRegistry.getInstance();
      let snapshot: TaskSnapshot;

      if (hasLoadedInstance(taskInfo)) {
        snapshot = TaskSerializationUtils.createTaskSnapshotFromTaskInfo(taskInfo);
      } else {
        const storedTask = taskInfo as StoredTaskInfo;
        const instanceId =
          storedTask.instanceRef.type === "reference"
            ? storedTask.instanceRef.instanceId
            : storedTask.instanceRef.instance.id;

        snapshot = {
          _version: 1,
          _timestamp: Date.now(),
          _entityType: "task",
          id: storedTask.id,
          instanceType: storedTask.instanceType,
          instanceId,
          workflowId: "",
          status: storedTask.status,
          submitTime: storedTask.submitTime,
          startTime: storedTask.startTime,
          completeTime: storedTask.completeTime,
          timeout: storedTask.timeout,
        };

        if (
          storedTask.instanceRef.type === "loaded" &&
          isThreadInstance(storedTask.instanceRef.instance)
        ) {
          snapshot.threadId = storedTask.instanceRef.instance.id;
          snapshot.workflowId = storedTask.instanceRef.instance.getWorkflowId();
        }

        if (storedTask.result) {
          snapshot.result = TaskSerializationUtils.serializeThreadResult(storedTask.result);
        }

        if (storedTask.error) {
          snapshot.error = ErrorSerializer.serialize(storedTask.error);
        }
      }

      const data = registry.serialize(snapshot);

      const metadata: TaskStorageMetadata = {
        taskId: snapshot.id,
        threadId: snapshot.threadId ?? snapshot.instanceId,
        workflowId: snapshot.workflowId,
        status: snapshot.status,
        submitTime: snapshot.submitTime,
        startTime: snapshot.startTime,
        completeTime: snapshot.completeTime,
        timeout: snapshot.timeout,
        error: snapshot.error?.message,
        errorStack: snapshot.error?.stack,
      };

      await this.storageCallback.save(taskId, data, metadata);
    } catch (error) {
      logger.warn(`Failed to persist task ${taskId}`, { error });
    }
  }

  /**
   * Delete a task from storage
   * @param taskId Task ID
   */
  private async unpersistTask(taskId: string): Promise<void> {
    if (!this.storageCallback) return;

    try {
      await this.storageCallback.delete(taskId);
    } catch (error) {
      logger.warn(`Failed to delete task ${taskId} from storage`, { error });
    }
  }

  /**
   * Register a task (unified method for both Agent and Thread)
   * @param instance Execution instance (AgentLoopEntity or ThreadEntity)
   * @param instanceType Execution instance type
   * @param manager Task manager
   * @param timeout Timeout period (in milliseconds)
   * @returns Task ID
   */
  register(
    instance: ExecutionInstance,
    instanceType: ExecutionInstanceType,
    manager: TaskManager,
    timeout?: number,
  ): string {
    const taskId = generateId();

    const taskInfo: TaskInfo = {
      id: taskId,
      instanceType,
      instance,
      status: "QUEUED",
      submitTime: now(),
      timeout,
    };

    this.tasks.set(taskId, taskInfo);
    this.taskManagers.set(taskId, manager);

    // Persist to storage (async, non-blocking)
    this.persistTask(taskId).catch(() => {});

    return taskId;
  }

  /**
   * Register a Thread task (convenience method for backward compatibility)
   * @param threadEntity Thread entity
   * @param manager Task manager
   * @param timeout Timeout period (in milliseconds)
   * @returns Task ID
   */
  registerThread(threadEntity: ThreadEntity, manager: TaskManager, timeout?: number): string {
    return this.register(threadEntity, "thread", manager, timeout);
  }

  /**
   * Update the task status to Running.
   * @param taskId Task ID
   */
  updateStatusToRunning(taskId: string): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "RUNNING";
      taskInfo.startTime = now();
      this.persistTask(taskId).catch(() => {});
    }
  }

  /**
   * Update the task status to Completed
   * @param taskId Task ID
   * @param result Execution result
   */
  updateStatusToCompleted(taskId: string, result: WorkflowExecutionResult): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "COMPLETED";
      taskInfo.completeTime = now();
      taskInfo.result = result;
      this.stats.completed++;
      this.persistTask(taskId).catch(() => {});
    }
  }

  /**
   * Update the task status to Failed.
   * @param taskId Task ID
   * @param error Error message
   */
  updateStatusToFailed(taskId: string, error: Error): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "FAILED";
      taskInfo.completeTime = now();
      taskInfo.error = error;
      this.stats.failed++;
      this.persistTask(taskId).catch(() => {});
    }
  }

  /**
   * Update the task status to Canceled
   * @param taskId Task ID
   */
  updateStatusToCancelled(taskId: string): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "CANCELLED";
      taskInfo.completeTime = now();
      this.stats.cancelled++;
      this.persistTask(taskId).catch(() => {});
    }
  }

  /**
   * Update the task status to Timeout
   * @param taskId Task ID
   */
  updateStatusToTimeout(taskId: string): void {
    const taskInfo = this.tasks.get(taskId);
    if (taskInfo) {
      taskInfo.status = "TIMEOUT";
      taskInfo.completeTime = now();
      this.stats.timeout++;
      this.persistTask(taskId).catch(() => {});
    }
  }

  /**
   * Update the instance for a stored task (restore from reference)
   * @param taskId Task ID
   * @param instance Execution instance
   * @returns True if the instance was updated
   */
  updateInstance(taskId: string, instance: ExecutionInstance): boolean {
    const taskInfo = this.tasks.get(taskId);
    if (!taskInfo) return false;

    if (isStoredTaskInfo(taskInfo)) {
      // Convert StoredTaskInfo to TaskInfo
      const taskInfoWithInstance: TaskInfo = {
        id: taskInfo.id,
        instanceType: taskInfo.instanceType,
        instance,
        status: taskInfo.status,
        submitTime: taskInfo.submitTime,
        startTime: taskInfo.startTime,
        completeTime: taskInfo.completeTime,
        result: taskInfo.result,
        error: taskInfo.error,
        timeout: taskInfo.timeout,
      };
      this.tasks.set(taskId, taskInfoWithInstance);
      return true;
    }

    // Already has loaded instance
    return false;
  }

  /**
   * Cancel a task (route to the appropriate manager)
   * @param taskId Task ID
   * @returns Whether the cancellation was successful
   */
  async cancelTask(taskId: string): Promise<boolean> {
    const manager = this.taskManagers.get(taskId);
    if (!manager) {
      return false;
    }

    const success = await manager.cancelTask(taskId);

    if (success) {
      this.updateStatusToCancelled(taskId);
      this.taskManagers.delete(taskId);
    }

    return success;
  }

  /**
   * Get task information
   * @param taskId Task ID
   * @returns Task information (null if not found or if instance is not loaded)
   */
  get(taskId: string): TaskInfo | null {
    const task = this.tasks.get(taskId);
    if (!task) return null;

    // Return only if the instance is loaded
    if (hasLoadedInstance(task)) {
      return task;
    }

    // Task exists but instance is not loaded (StoredTaskInfo)
    return null;
  }

  /**
   * Get stored task information (including tasks with unloaded instances)
   * @param taskId Task ID
   * @returns Stored task information
   */
  getStored(taskId: string): StoredTaskInfo | null {
    const task = this.tasks.get(taskId);
    if (!task) return null;

    if (isStoredTaskInfo(task)) {
      return task;
    }

    // Convert TaskInfo to StoredTaskInfo
    return {
      id: task.id,
      instanceType: task.instanceType,
      instanceRef: { type: "loaded", instance: task.instance },
      status: task.status,
      submitTime: task.submitTime,
      startTime: task.startTime,
      completeTime: task.completeTime,
      result: task.result,
      error: task.error,
      timeout: task.timeout,
    };
  }

  /**
   * Check if the task exists
   * @param taskId Task ID
   * @returns Whether it exists
   */
  has(taskId: string): boolean {
    return this.tasks.has(taskId);
  }

  /**
   * Get all tasks with loaded instances
   * @returns Array of all task information with loaded instances
   */
  getAll(): TaskInfo[] {
    return Array.from(this.tasks.values()).filter(hasLoadedInstance);
  }

  /**
   * Get all stored tasks (including those with unloaded instances)
   * @returns Array of all stored task information
   */
  getAllStored(): StoredTaskInfo[] {
    return Array.from(this.tasks.values()).map(task => {
      if (isStoredTaskInfo(task)) {
        return task;
      }
      return {
        id: task.id,
        instanceType: task.instanceType,
        instanceRef: { type: "loaded", instance: task.instance },
        status: task.status,
        submitTime: task.submitTime,
        startTime: task.startTime,
        completeTime: task.completeTime,
        result: task.result,
        error: task.error,
        timeout: task.timeout,
      };
    });
  }

  /**
   * Get tasks based on the status (only with loaded instances)
   * @param status Task status
   * @returns Array of tasks in the specified status
   */
  getByStatus(status: TaskStatus): TaskInfo[] {
    return Array.from(this.tasks.values())
      .filter(hasLoadedInstance)
      .filter(task => task.status === status);
  }

  /**
   * Get a task by thread ID
   * @param threadId Thread ID
   * @returns Task information
   */
  getByThreadId(threadId: string): TaskInfo | null {
    return (
      this.getAll().find(task => {
        if (task.instanceType === "thread" && isThreadInstance(task.instance)) {
          return task.instance.id === threadId;
        }
        return false;
      }) || null
    );
  }

  /**
   * Get a task by instance ID (works for both Agent and Thread)
   * @param instanceId Instance ID
   * @returns Task information
   */
  getByInstanceId(instanceId: string): TaskInfo | null {
    return this.getAll().find(task => task.instance.id === instanceId) || null;
  }

  /**
   * Delete Task
   * @param taskId Task ID
   * @returns Whether the deletion was successful
   */
  async delete(taskId: string): Promise<boolean> {
    this.taskManagers.delete(taskId);
    const result = this.tasks.delete(taskId);

    if (result) {
      await this.unpersistTask(taskId);
    }

    return result;
  }

  /**
   * Clean up expired tasks
   * @param retentionTime Retention time in milliseconds, default is 1 hour
   * @returns Number of tasks that were cleaned up
   */
  async cleanup(retentionTime: number = 60 * 60 * 1000): Promise<number> {
    const currentTime = now();
    let cleanedCount = 0;

    for (const [taskId, taskInfo] of this.tasks.entries()) {
      // Only clean up tasks that have been completed, failed, canceled, or timed out.
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
        await this.unpersistTask(taskId);
        cleanedCount++;
      }
    }

    return cleanedCount;
  }

  /**
   * Get statistical information
   * @returns Statistical information
   */
  getStats() {
    const tasks = Array.from(this.tasks.values());

    return {
      total: tasks.length,
      queued: tasks.filter(t => t.status === "QUEUED").length,
      running: tasks.filter(t => t.status === "RUNNING").length,
      completed: this.stats.completed,
      failed: this.stats.failed,
      cancelled: this.stats.cancelled,
      timeout: this.stats.timeout,
    };
  }

  /**
   * Clear all tasks
   */
  async clear(): Promise<void> {
    // Clear from storage if persistence is enabled
    if (this.storageCallback) {
      try {
        await this.storageCallback.clear();
      } catch (error) {
        logger.warn("Failed to clear task storage", { error });
      }
    }

    this.tasks.clear();
    this.taskManagers.clear();
    this.stats = {
      completed: 0,
      failed: 0,
      cancelled: 0,
      timeout: 0,
    };
  }

  /**
   * Get the number of tasks
   * @returns The number of tasks
   */
  size(): number {
    return this.tasks.size;
  }

  /**
   * Close the registry and release resources
   */
  async close(): Promise<void> {
    if (this.storageCallback) {
      try {
        await this.storageCallback.close();
      } catch (error) {
        logger.warn("Failed to close task storage", { error });
      }
    }
    this.initialized = false;
  }

  /**
   * Check if persistence is enabled
   * @returns Whether persistence is enabled
   */
  isPersistenceEnabled(): boolean {
    return this.storageCallback !== undefined;
  }

  /**
   * Get task snapshot from storage (for tasks loaded from persistence)
   * @param taskId Task ID
   * @returns Task snapshot or null
   */
  async getTaskSnapshot(taskId: string): Promise<TaskSnapshot | null> {
    if (!this.storageCallback) return null;

    const data = await this.storageCallback.load(taskId);
    if (!data) return null;

    const registry = SerializationRegistry.getInstance();
    return registry.deserialize<TaskSnapshot>("task", data);
  }
}
