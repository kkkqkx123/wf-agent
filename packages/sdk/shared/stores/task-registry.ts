/**
 * TaskRegistry - Task Registry (DI Container Managed Service)
 *
 * Responsibilities:
 * - Stores and manages information about all tasks
 * - Tracks task status, execution results, timestamps, etc.
 * - Provides functionality for querying and cleaning up tasks
 * - Routes task operations to the appropriate managers
 * - Supports optional persistence through TaskStorageAdapter
 *
 * Design Principles:
 * - Managed by DI container (one instance per SDK instance)
 * - Unified task management for Agent and Workflow executions
 * - Supports regular cleaning of expired tasks
 * - Provides manager routing functionality
 * - Optional persistence layer for task data
 * - Initialized automatically by DI container with proper async initialization
 */

import { generateId } from "../../utils/index.js";
import { now } from "@wf-agent/common-utils";
import type { TaskStorageMetadata } from "@wf-agent/types";
import {
  TaskStatus,
  type TaskInfo,
  type StoredTaskInfo,
  type ExecutionInstance,
  type ExecutionInstanceType,
  isWorkflowExecutionInstance,
  hasLoadedInstance,
  isStoredTaskInfo,
} from "../types/index.js";
import type { TaskStorageAdapter } from "@wf-agent/storage";
import { ErrorCodec, StateCodec } from "@wf-agent/common-utils";
import { type TaskSnapshot, TaskSerializationUtils } from "../types/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TaskRegistry" });

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
 * Task status update metadata
 */
export interface TaskStatusUpdateMetadata {
  /** Execution result (for COMPLETED status) */
  result?: unknown;
  /** Error object (for FAILED status) */
  error?: Error;
  /** Execution duration in milliseconds */
  duration?: number;
}

/**
 * TaskRegistry Configuration
 */
export interface TaskRegistryConfig {
  /** Optional storage adapter for persistence */
  storageAdapter?: TaskStorageAdapter;
  /**
   * Persistence mode:
   * - 'none': No persistence (in-memory only)
   * - 'auto': Automatically persist on status changes (default with adapter)
   * - 'manual': Explicit persistence via persistNow()
   */
  persistenceMode?: 'none' | 'auto' | 'manual';
}

/**
 * TaskRegistry - Task Registry (managed by DI container)
 *
 * Lifecycle states:
 * - 'uninitialized': Created but not yet initialized
 * - 'initializing': Currently initializing storage
 * - 'initialized': Ready to use
 * - 'error': Initialization failed
 *
 * Supports both in-memory and persistent storage modes:
 * - Without storageAdapter: Pure in-memory mode (automatic initialization)
 * - With storageAdapter: Persistent mode (requires async initialize() call)
 */
export class TaskRegistry {
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
   * Storage adapter for persistence (optional)
   */
  private storageAdapter?: TaskStorageAdapter;

  /**
   * Persistence mode: 'none' | 'auto' | 'manual'
   */
  private persistenceMode: 'none' | 'auto' | 'manual' = 'none';

  /**
   * Lifecycle state: 'uninitialized' | 'initializing' | 'initialized' | 'error'
   */
  private state: 'uninitialized' | 'initializing' | 'initialized' | 'error' = 'uninitialized';

  /**
   * Task statistics
   */
  private stats: {
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
   * Constructor - synchronous setup only
   * If no storageAdapter is provided, registry is immediately ready for use (in-memory mode)
   * If storageAdapter is provided, must call initialize() before use
   *
   * @param config Configuration options
   */
  constructor(config?: TaskRegistryConfig) {
    if (config?.storageAdapter) {
      this.storageAdapter = config.storageAdapter;
      this.persistenceMode = config.persistenceMode ?? 'auto';
      // Leave state as 'uninitialized', requires explicit initialize() call
    } else {
      // No storage adapter, immediately ready for in-memory use
      this.persistenceMode = 'none';
      this.state = 'initialized';
    }
  }

  /**
   * Initialize the registry with storage persistence
   * Must be called if storageAdapter was provided in constructor
   *
   * @throws Error if registry is already initialized or in error state
   */
  async initialize(): Promise<void> {
    // Check current state
    if (this.state === 'initialized') {
      logger.warn("TaskRegistry already initialized");
      return;
    }

    if (this.state === 'initializing') {
      throw new Error("TaskRegistry initialization already in progress");
    }

    if (this.state === 'error') {
      throw new Error("TaskRegistry is in error state, cannot reinitialize");
    }

    // No storage adapter, no async init needed
    if (!this.storageAdapter) {
      this.state = 'initialized';
      return;
    }

    this.state = 'initializing';

    try {
      // Initialize storage adapter
      await this.storageAdapter.initialize();
      logger.debug("TaskRegistry storage adapter initialized");

      // Load existing tasks from storage
      await this.loadFromStorage();

      this.state = 'initialized';
      logger.info("TaskRegistry initialized with persistence enabled");
    } catch (error) {
      this.state = 'error';
      logger.error("TaskRegistry initialization failed", { error });
      throw error;
    }
  }

  /**
   * Check if registry is initialized and ready for use
   */
  isInitialized(): boolean {
    return this.state === 'initialized';
  }

  /**
   * Load tasks from storage
   */
  private async loadFromStorage(): Promise<void> {
    if (!this.storageAdapter) return;

    try {
      const taskIds = await this.storageAdapter.list();
      const codec = new StateCodec();

      for (const taskId of taskIds) {
        const data = await this.storageAdapter.load(taskId);
        if (data) {
          const snapshot = await codec.deserialize<TaskSnapshot>(data);
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
            storedTask.result = snapshot.result;
          }

          if (snapshot.error) {
            storedTask.error = ErrorCodec.deserialize(snapshot.error);
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
   * @internal Internal method, called by updateStatus() when persistenceMode='auto'
   */
  private async persistTask(taskId: string): Promise<void> {
    if (this.persistenceMode === 'none' || !this.storageAdapter) return;

    const taskInfo = this.tasks.get(taskId);
    if (!taskInfo) return;

    try {
      const codec = new StateCodec();
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
          isWorkflowExecutionInstance(storedTask.instanceRef.instance)
        ) {
          snapshot.executionId = storedTask.instanceRef.instance.id;
          snapshot.workflowId = storedTask.instanceRef.instance.getWorkflowId();
        }

        if (storedTask.result) {
          snapshot.result = storedTask.result;
        }

        if (storedTask.error) {
          snapshot.error = ErrorCodec.serialize(storedTask.error);
        }
      }

      const data = await codec.serialize(snapshot);

      const metadata: TaskStorageMetadata = {
        taskId: snapshot.id,
        executionId: snapshot.executionId ?? snapshot.instanceId,
        workflowId: snapshot.workflowId,
        status: snapshot.status,
        submitTime: snapshot.submitTime,
        startTime: snapshot.startTime,
        completeTime: snapshot.completeTime,
        timeout: snapshot.timeout,
        error: snapshot.error?.message,
        errorStack: snapshot.error?.stack,
      };

      await this.storageAdapter.save(taskId, data, metadata);
    } catch (error) {
      logger.warn(`Failed to persist task ${taskId}`, { error });
    }
  }

  /**
   * Delete a task from storage
   * @param taskId Task ID
   */
  private async unpersistTask(taskId: string): Promise<void> {
    if (!this.storageAdapter) return;

    try {
      await this.storageAdapter.delete(taskId);
    } catch (error) {
      logger.warn(`Failed to delete task ${taskId} from storage`, { error });
    }
  }

  /**
   * Register a task (unified method for both Agent and WorkflowExecution)
   * @param instance Execution instance (AgentLoopEntity or WorkflowExecutionEntity)
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
   * Update task status with optional metadata
   *
   * Unified method for all status transitions. Automatically:
   * - Records timestamps (startTime for RUNNING, completeTime for terminal states)
   * - Updates statistics counters (completed, failed, cancelled, timeout)
   * - Persists to storage (if persistenceMode='auto')
   *
   * @param taskId Task ID
   * @param status New task status
   * @param metadata Optional metadata (result, error, duration)
   */
  updateStatus(taskId: string, status: TaskStatus, metadata?: TaskStatusUpdateMetadata): void {
    const taskInfo = this.tasks.get(taskId);
    if (!taskInfo) return;

    // Update status
    taskInfo.status = status;

    // Record start time for RUNNING state
    if (status === "RUNNING") {
      taskInfo.startTime = now();
    }

    // Record completion time and metadata for terminal states
    if (status === "COMPLETED" || status === "FAILED" || status === "CANCELLED" || status === "TIMEOUT") {
      taskInfo.completeTime = now();

      // Apply metadata
      if (metadata?.result !== undefined) {
        taskInfo.result = metadata.result;
      }
      if (metadata?.error !== undefined) {
        taskInfo.error = metadata.error;
      }

      // Update statistics
      switch (status) {
        case "COMPLETED":
          this.stats.completed++;
          break;
        case "FAILED":
          this.stats.failed++;
          break;
        case "CANCELLED":
          this.stats.cancelled++;
          break;
        case "TIMEOUT":
          this.stats.timeout++;
          break;
      }
    }

    // Persist if in auto mode
    if (this.persistenceMode === "auto") {
      this.persistTask(taskId).catch(() => {});
    }
  }

  /**
   * Explicitly persist a task to storage
   * Only applicable when persistenceMode='manual'
   *
   * @param taskId Task ID
   */
  async persistNow(taskId: string): Promise<void> {
    if (this.persistenceMode === "manual" || this.persistenceMode === "auto") {
      await this.persistTask(taskId);
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
      this.updateStatus(taskId, "CANCELLED");
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
   * Get a task by execution ID
   * @param executionId Execution ID
   * @returns Task information
   */
  getByExecutionId(executionId: string): TaskInfo | null {
    return (
      this.getAll().find(task => {
        if (
          task.instanceType === "workflowExecution" &&
          isWorkflowExecutionInstance(task.instance)
        ) {
          return task.instance.id === executionId;
        }
        // Support agent type as well
        if (task.instanceType === "agent") {
          return task.instance.id === executionId;
        }
        return false;
      }) || null
    );
  }

  /**
   * Get a task by instance ID (works for both Agent and WorkflowExecution)
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
    if (this.storageAdapter) {
      try {
        await this.storageAdapter.clear();
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
    if (this.storageAdapter) {
      try {
        await this.storageAdapter.close();
      } catch (error) {
        logger.warn("Failed to close task storage", { error });
      }
    }
    this.state = 'uninitialized';
  }

  /**
   * Check if persistence is enabled
   * @returns Whether persistence is enabled
   */
  isPersistenceEnabled(): boolean {
    return this.storageAdapter !== undefined;
  }

  /**
   * Get task snapshot from storage (for tasks loaded from persistence)
   * @param taskId Task ID
   * @returns Task snapshot or null
   */
  async getTaskSnapshot(taskId: string): Promise<TaskSnapshot | null> {
    if (!this.storageAdapter) return null;

    const data = await this.storageAdapter.load(taskId);
    if (!data) return null;

    const codec = new StateCodec();
    return codec.deserialize<TaskSnapshot>(data);
  }
}
