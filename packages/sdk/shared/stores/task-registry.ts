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
   * - 'auto-batch': Asynchronously batch persist on status changes (non-blocking)
   * - 'manual': Explicit persistence via persistNow() or flush()
   * @default 'auto-batch' if storageAdapter provided, 'none' otherwise
   */
  persistenceMode?: 'none' | 'auto-batch' | 'manual';
  /**
   * Batch persistence configuration
   */
  persistenceBatchConfig?: {
    /** Number of tasks to batch before flushing (default: 100) */
    batchSize?: number;
    /** Max wait time before flushing (default: 5000ms) */
    maxWaitTime?: number;
  };
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
   * Active Task Pool
   * Stores tasks that are actively tracked in memory
   */
  private activePool: Map<string, TaskInfo> = new Map();

  /**
   * Archived Task References
   * Stores minimal references to tasks loaded from persistence
   */
  private archivedRefs: Map<string, StoredTaskInfo> = new Map();

  /**
   * Task ID to Manager Mapping
   */
  private taskManagers: Map<string, TaskManager> = new Map();

  /**
   * Storage adapter for persistence (optional)
   */
  private storageAdapter?: TaskStorageAdapter;

  /**
   * Persistence mode: 'none' | 'auto-batch' | 'manual'
   */
  private persistenceMode: 'none' | 'auto-batch' | 'manual' = 'none';

  /**
   * Batch persistence configuration
   */
  private batchConfig: Required<Exclude<TaskRegistryConfig['persistenceBatchConfig'], undefined>> = {
    batchSize: 100,
    maxWaitTime: 5000,
  };

  /**
   * Tasks pending batch persistence (tracks which tasks need flushing)
   */
  private pendingPersistence: Set<string> = new Set();

  /**
   * Batch persistence timer
   */
  private batchPersistenceTimer?: NodeJS.Timeout;

  /**
   * Lifecycle state: 'uninitialized' | 'initializing' | 'initialized' | 'error'
   */
  private state: 'uninitialized' | 'initializing' | 'initialized' | 'error' = 'uninitialized';

  /**
   * Task statistics - atomic operation ready
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
      this.persistenceMode = config.persistenceMode ?? 'auto-batch';
      // Merge batch config
      if (config.persistenceBatchConfig) {
        this.batchConfig = {
          batchSize: config.persistenceBatchConfig.batchSize ?? 100,
          maxWaitTime: config.persistenceBatchConfig.maxWaitTime ?? 5000,
        };
      }
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
   * Load tasks from storage into archived references
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
            deadlineTime: (snapshot as any).deadlineTime,
            timeoutPolicy: (snapshot as any).timeoutPolicy ?? 'cancel',
          };

          if (snapshot.result) {
            storedTask.result = snapshot.result;
          }

          if (snapshot.error) {
            storedTask.error = ErrorCodec.deserialize(snapshot.error);
          }

          // Store in archived refs, not active pool
          this.archivedRefs.set(taskId, storedTask);
        }
      }
    } catch (error) {
      logger.warn("Failed to load tasks from storage", { error });
    }
  }

  /**
   * Persist a single task to storage
   * @param taskId Task ID
   * @internal Internal method, called by updateStatus() when persistenceMode='auto-batch'
   */
  private async persistTask(taskId: string): Promise<void> {
    if (this.persistenceMode === 'none' || !this.storageAdapter) return;

    // Get task from active pool or archived refs
    const taskInfo = this.activePool.get(taskId);
    const archivedInfo = this.archivedRefs.get(taskId);
    const task = taskInfo || archivedInfo;

    if (!task) return;

    try {
      const codec = new StateCodec();
      let snapshot: TaskSnapshot;

      if (taskInfo && hasLoadedInstance(taskInfo)) {
        snapshot = TaskSerializationUtils.createTaskSnapshotFromTaskInfo(taskInfo);
        // Preserve deadline and timeout policy
        (snapshot as any).deadlineTime = taskInfo.deadlineTime;
        (snapshot as any).timeoutPolicy = taskInfo.timeoutPolicy ?? 'cancel';
      } else if (archivedInfo) {
        const storedTask = archivedInfo;
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
        } as any;

        // Preserve deadline and timeout policy
        (snapshot as any).deadlineTime = storedTask.deadlineTime;
        (snapshot as any).timeoutPolicy = storedTask.timeoutPolicy ?? 'cancel';

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
      } else {
        return;
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
   * Queue a task for batch persistence (non-blocking)
   * The actual persistence happens asynchronously in batches
   * @internal
   */
  private queueForBatchPersistence(taskId: string): void {
    if (this.persistenceMode !== 'auto-batch' || !this.storageAdapter) return;

    this.pendingPersistence.add(taskId);

    // If batch is full, flush immediately
    if (this.pendingPersistence.size >= this.batchConfig.batchSize) {
      this.flushBatchPersistence().catch(error => {
        logger.warn("Failed to flush batch persistence", { error });
      });
      return;
    }

    // If timer not set, schedule one
    if (!this.batchPersistenceTimer) {
      this.batchPersistenceTimer = setTimeout(() => {
        this.flushBatchPersistence().catch(error => {
          logger.warn("Failed to flush batch persistence", { error });
        });
      }, this.batchConfig.maxWaitTime);
    }
  }

  /**
   * Flush all pending tasks to storage
   * Non-blocking, runs asynchronously
   */
  async flushBatchPersistence(): Promise<void> {
    if (this.pendingPersistence.size === 0) return;

    // Clear timer
    if (this.batchPersistenceTimer) {
      clearTimeout(this.batchPersistenceTimer);
      this.batchPersistenceTimer = undefined;
    }

    const taskIds = Array.from(this.pendingPersistence);
    this.pendingPersistence.clear();

    logger.debug("Flushing batch persistence", { count: taskIds.length });

    // Persist each task in batch (fire-and-forget to avoid blocking)
    const promises = taskIds.map(taskId =>
      this.persistTask(taskId).catch(error => {
        logger.warn(`Failed to persist task ${taskId}`, { error });
      })
    );

    // Wait for all to complete
    await Promise.all(promises);
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
   * Tasks are created in the active pool
   * @param instance Execution instance (AgentLoopEntity or WorkflowExecutionEntity)
   * @param instanceType Execution instance type
   * @param manager Task manager
   * @param timeout Timeout period (in milliseconds)
   * @param timeoutPolicy Timeout policy for recovery (default: 'cancel')
   * @returns Task ID
   */
  register(
    instance: ExecutionInstance,
    instanceType: ExecutionInstanceType,
    manager: TaskManager,
    timeout?: number,
    timeoutPolicy?: string,
  ): string {
    const taskId = generateId();
    const submitTime = now();
    const deadlineTime = timeout ? submitTime + timeout : undefined;

    const taskInfo: TaskInfo = {
      id: taskId,
      instanceType,
      instance,
      status: "QUEUED",
      submitTime,
      timeout,
      deadlineTime,
      timeoutPolicy: (timeoutPolicy as any) ?? 'cancel',
    };

    // Store in active pool, not archived refs
    this.activePool.set(taskId, taskInfo);
    this.taskManagers.set(taskId, manager);

    // Queue for batch persistence (non-blocking)
    this.queueForBatchPersistence(taskId);

    return taskId;
  }

  /**
   * Update task status with optional metadata
   *
   * Unified method for all status transitions. Automatically:
   * - Records timestamps (startTime for RUNNING, completeTime for terminal states)
   * - Updates statistics counters (completed, failed, cancelled, timeout)
   * - Queues for batch persistence (if persistenceMode='auto-batch')
   *
   * @param taskId Task ID
   * @param status New task status
   * @param metadata Optional metadata (result, error, duration)
   */
  updateStatus(taskId: string, status: TaskStatus, metadata?: TaskStatusUpdateMetadata): void {
    const taskInfo = this.activePool.get(taskId);
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

      // Update statistics (atomic operation)
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

    // Queue for batch persistence (non-blocking)
    this.queueForBatchPersistence(taskId);
  }

  /**
   * Explicitly persist a task to storage
   * - In 'auto-batch' mode: adds to batch queue (still non-blocking)
   * - In 'manual' mode: persists immediately
   *
   * @param taskId Task ID
   */
  async persistNow(taskId: string): Promise<void> {
    if (this.persistenceMode === "manual") {
      await this.persistTask(taskId);
    } else if (this.persistenceMode === "auto-batch") {
      this.queueForBatchPersistence(taskId);
    }
  }

  /**
   * Flush all pending batch persistence operations
   * Ensures all queued tasks are persisted to storage
   * Only applicable when persistenceMode='auto-batch' or 'manual'
   *
   * @returns Promise that resolves when all pending tasks are persisted
   */
  async flush(): Promise<void> {
    if (this.persistenceMode === 'auto-batch') {
      await this.flushBatchPersistence();
    }
  }

  /**
   * Update the instance for a stored task (restore from reference into active pool)
   * @param taskId Task ID
   * @param instance Execution instance
   * @returns True if the instance was updated
   */
  updateInstance(taskId: string, instance: ExecutionInstance): boolean {
    const archivedInfo = this.archivedRefs.get(taskId);
    if (!archivedInfo) return false;

    // Convert StoredTaskInfo to TaskInfo and move to active pool
    const taskInfoWithInstance: TaskInfo = {
      id: archivedInfo.id,
      instanceType: archivedInfo.instanceType,
      instance,
      status: archivedInfo.status,
      submitTime: archivedInfo.submitTime,
      startTime: archivedInfo.startTime,
      completeTime: archivedInfo.completeTime,
      result: archivedInfo.result,
      error: archivedInfo.error,
      timeout: archivedInfo.timeout,
    };
    this.activePool.set(taskId, taskInfoWithInstance);
    this.archivedRefs.delete(taskId);
    return true;
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
   * Get task information (only from active pool with loaded instances)
   * @param taskId Task ID
   * @returns Task information (null if not found or not in active pool)
   */
  get(taskId: string): TaskInfo | null {
    return this.activePool.get(taskId) || null;
  }

  /**
   * Get stored task information (including tasks with unloaded instances from archived refs)
   * @param taskId Task ID
   * @returns Stored task information
   */
  getStored(taskId: string): StoredTaskInfo | null {
    const activeTask = this.activePool.get(taskId);
    const archivedTask = this.archivedRefs.get(taskId);

    if (activeTask) {
      // Convert active TaskInfo to StoredTaskInfo
      return {
        id: activeTask.id,
        instanceType: activeTask.instanceType,
        instanceRef: { type: "loaded", instance: activeTask.instance },
        status: activeTask.status,
        submitTime: activeTask.submitTime,
        startTime: activeTask.startTime,
        completeTime: activeTask.completeTime,
        result: activeTask.result,
        error: activeTask.error,
        timeout: activeTask.timeout,
      };
    }

    return archivedTask || null;
  }

  /**
   * Check if the task exists (in active pool or archived refs)
   * @param taskId Task ID
   * @returns Whether it exists
   */
  has(taskId: string): boolean {
    return this.activePool.has(taskId) || this.archivedRefs.has(taskId);
  }

  /**
   * Get all tasks with loaded instances (active pool only)
   * @returns Array of all task information with loaded instances
   */
  getAll(): TaskInfo[] {
    return Array.from(this.activePool.values());
  }

  /**
   * Get all stored tasks (active pool + archived refs)
   * @returns Array of all stored task information
   */
  getAllStored(): StoredTaskInfo[] {
    const result: StoredTaskInfo[] = [];

    // Add all active tasks converted to StoredTaskInfo
    for (const task of this.activePool.values()) {
      result.push({
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
      });
    }

    // Add all archived tasks
    for (const task of this.archivedRefs.values()) {
      result.push(task);
    }

    return result;
  }

  /**
   * Get tasks based on the status (only with loaded instances in active pool)
   * @param status Task status
   * @returns Array of tasks in the specified status
   */
  getByStatus(status: TaskStatus): TaskInfo[] {
    return Array.from(this.activePool.values()).filter(task => task.status === status);
  }

  /**
   * Get a task by execution ID (searches active pool)
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
   * Get a task by instance ID (searches active pool, works for both Agent and WorkflowExecution)
   * @param instanceId Instance ID
   * @returns Task information
   */
  getByInstanceId(instanceId: string): TaskInfo | null {
    return this.getAll().find(task => task.instance.id === instanceId) || null;
  }

  /**
   * Delete a task (from both active pool and archived refs)
   * @param taskId Task ID
   * @returns Whether the deletion was successful
   */
  async delete(taskId: string): Promise<boolean> {
    this.taskManagers.delete(taskId);
    const activeDeleted = this.activePool.delete(taskId);
    const archivedDeleted = this.archivedRefs.delete(taskId);

    if (activeDeleted || archivedDeleted) {
      await this.unpersistTask(taskId);
      return true;
    }

    return false;
  }

  /**
   * Clean up expired tasks (only from active pool - completed, failed, cancelled, timeout)
   * @param retentionTime Retention time in milliseconds, default is 1 hour
   * @returns Number of tasks that were cleaned up
   */
  async cleanup(retentionTime: number = 60 * 60 * 1000): Promise<number> {
    const currentTime = now();
    let cleanedCount = 0;

    // Clean up from active pool
    for (const [taskId, taskInfo] of this.activePool.entries()) {
      if (
        (taskInfo.status === "COMPLETED" ||
          taskInfo.status === "FAILED" ||
          taskInfo.status === "CANCELLED" ||
          taskInfo.status === "TIMEOUT") &&
        taskInfo.completeTime &&
        currentTime - taskInfo.completeTime > retentionTime
      ) {
        this.activePool.delete(taskId);
        this.taskManagers.delete(taskId);
        await this.unpersistTask(taskId);
        cleanedCount++;
      }
    }

    // Clean up from archived refs
    for (const [taskId, taskInfo] of this.archivedRefs.entries()) {
      if (
        (taskInfo.status === "COMPLETED" ||
          taskInfo.status === "FAILED" ||
          taskInfo.status === "CANCELLED" ||
          taskInfo.status === "TIMEOUT") &&
        taskInfo.completeTime &&
        currentTime - taskInfo.completeTime > retentionTime
      ) {
        this.archivedRefs.delete(taskId);
        await this.unpersistTask(taskId);
        cleanedCount++;
      }
    }

    return cleanedCount;
  }

  /**
   * Clear all tasks (active pool, archived refs, and storage)
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

    this.activePool.clear();
    this.archivedRefs.clear();
    this.taskManagers.clear();
    this.stats = {
      completed: 0,
      failed: 0,
      cancelled: 0,
      timeout: 0,
    };
  }

  /**
   * Get the number of tasks (active pool + archived refs)
   * @returns The number of tasks
   */
  size(): number {
    return this.activePool.size + this.archivedRefs.size;
  }

  /**
   * Get statistical information (only from active pool)
   * @returns Statistical information
   */
  getStats() {
    const tasks = Array.from(this.activePool.values());

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
   * Close the registry and release resources
   */
  async close(): Promise<void> {
    // Flush any pending persistence
    if (this.persistenceMode === 'auto-batch') {
      await this.flushBatchPersistence();
    }

    // Clear timer
    if (this.batchPersistenceTimer) {
      clearTimeout(this.batchPersistenceTimer);
      this.batchPersistenceTimer = undefined;
    }

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
