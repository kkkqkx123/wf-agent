/**
 * ThreadExecutionContext - Thread Execution Manager
 *
 * Responsibilities:
 * - Manages the execution of ThreadEntity instances
 * - Provides unified interface for sync and async execution
 * - Coordinates with ExecutionPool and ExecutionQueue
 * - Handles task cancellation and status tracking
 *
 * Design Principles:
 * - Unified execution management for Thread instances
 * - Uses shared Pool/Queue infrastructure
 * - Independent configuration from Agent execution
 * - Fault isolation from Agent execution
 */

import { TaskRegistry, type TaskManager } from "../stores/task/task-registry.js";
import {
  ExecutionPool,
  type Executor,
  type ExecutorFactory,
} from "../../core/execution/execution-pool.js";
import {
  ExecutionQueue,
  type ExecutionResult,
  type TaskSubmissionResult,
} from "../../core/execution/execution-queue.js";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import type { ThreadEntity } from "../entities/thread-entity.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import type {
  PoolStats,
  ExecutionPoolConfig,
  ExecutionInstanceType,
} from "../../core/types/index.js";

/**
 * Typed execution result
 * Generic interface for execution results with specific result type
 */
export interface TypedExecutionResult<T> extends ExecutionResult {
  /** Typed execution result */
  result?: T;
}

/**
 * Base dependencies for execution managers
 */
export interface BaseExecutionManagerDependencies<T, R> {
  /** Event manager */
  eventManager: EventRegistry;
  /** Executor factory */
  executorFactory: ExecutorFactory<T>;
  /** Pool configuration */
  poolConfig?: ExecutionPoolConfig;
  /** Pool identifier (optional, for multi-pool scenarios) */
  poolId?: string;
  /** Execute function */
  executeFn: (executor: Executor<T>, instance: T) => Promise<R>;
  /** Instance type identifier */
  instanceType: ExecutionInstanceType;
}

/**
 * ThreadExecutionContext dependencies
 */
export type ThreadExecutionManagerDependencies = BaseExecutionManagerDependencies<
  ThreadEntity,
  WorkflowExecutionResult
>;

/**
 * Default Thread pool configuration
 */
const DEFAULT_THREAD_POOL_CONFIG: ExecutionPoolConfig = {
  minExecutors: 1,
  maxExecutors: 10,
  idleTimeout: 30000,
  defaultTimeout: 60000,
};

/**
 * Default pool ID for thread execution
 */
const DEFAULT_THREAD_POOL_ID = "thread-execution-pool";

/**
 * ThreadExecutionContext - Thread Execution Manager
 *
 * Provides unified execution management for ThreadEntity instances.
 * Uses shared Pool/Queue infrastructure for resource management.
 */
export class ThreadExecutionContext implements TaskManager {
  /** Task registry */
  private taskRegistry: TaskRegistry;

  /** Pool service */
  private poolService: ExecutionPool<ThreadEntity>;

  /** Queue manager */
  private queueManager: ExecutionQueue<ThreadEntity>;

  /** Event manager */
  private eventManager: EventRegistry;

  /** Execute function */
  private executeFn: (
    executor: Executor<ThreadEntity>,
    instance: ThreadEntity,
  ) => Promise<WorkflowExecutionResult>;

  /** Instance type */
  private instanceType: ExecutionInstanceType;

  /**
   * Constructor
   * @param deps Dependencies
   */
  constructor(deps: ThreadExecutionManagerDependencies) {
    this.taskRegistry = TaskRegistry.getInstance();
    this.eventManager = deps.eventManager;
    this.executeFn = deps.executeFn;
    this.instanceType = deps.instanceType ?? "thread";

    const poolConfig = deps.poolConfig ?? DEFAULT_THREAD_POOL_CONFIG;
    const poolId = deps.poolId ?? DEFAULT_THREAD_POOL_ID;

    // Create pool service with configurable pool ID
    this.poolService = ExecutionPool.getInstance<ThreadEntity>(
      poolId,
      deps.executorFactory,
      poolConfig,
    );

    // Create queue manager
    this.queueManager = new ExecutionQueue<ThreadEntity>(
      this.taskRegistry,
      this.poolService,
      this.eventManager,
      async (executor: Executor<ThreadEntity>, instance: ThreadEntity) => {
        return this.executeFn(executor, instance);
      },
    );
  }

  /**
   * Execute Thread synchronously
   * @param threadEntity Thread entity to execute
   * @param timeout Optional timeout in milliseconds
   * @returns Execution result
   */
  async executeSync(
    threadEntity: ThreadEntity,
    timeout?: number,
  ): Promise<TypedExecutionResult<WorkflowExecutionResult>> {
    // Register task
    const taskId = this.taskRegistry.register(threadEntity, this.instanceType, this, timeout);

    // Submit to queue
    const result = await this.queueManager.submitSync(
      taskId,
      threadEntity,
      this.instanceType,
      timeout,
    );

    return {
      ...result,
      result: result.threadResult as WorkflowExecutionResult,
    };
  }

  /**
   * Execute Thread asynchronously
   * @param threadEntity Thread entity to execute
   * @param timeout Optional timeout in milliseconds
   * @returns Task submission result
   */
  executeAsync(threadEntity: ThreadEntity, timeout?: number): TaskSubmissionResult {
    // Register task
    const taskId = this.taskRegistry.register(threadEntity, this.instanceType, this, timeout);

    // Submit to queue
    return this.queueManager.submitAsync(taskId, threadEntity, this.instanceType, timeout);
  }

  /**
   * Cancel a task (implements TaskManager interface)
   * @param taskId Task ID
   * @returns Whether cancellation was successful
   */
  async cancelTask(taskId: string): Promise<boolean> {
    return this.queueManager.cancelTask(taskId);
  }

  /**
   * Get task status (implements TaskManager interface)
   * @param taskId Task ID
   * @returns Task info or null
   */
  getTaskStatus(taskId: string) {
    return this.taskRegistry.get(taskId);
  }

  /**
   * Get pool statistics
   * @returns Pool statistics
   */
  getPoolStats() {
    return this.poolService.getStats();
  }

  /**
   * Get queue statistics
   * @returns Queue statistics
   */
  getQueueStats() {
    return this.queueManager.getQueueStats();
  }

  /**
   * Wait for all tasks to complete
   */
  async drain(): Promise<void> {
    await this.queueManager.drain();
  }

  /**
   * Clear the queue
   */
  clearQueue(): void {
    this.queueManager.clear();
  }

  /**
   * Shutdown the manager
   */
  async shutdown(): Promise<void> {
    await this.queueManager.drain();
    await this.poolService.shutdown();
  }
}
