/**
 * ThreadPool - Thread Pool Service (Global Singleton Service)
 *
 * Responsibilities:
 * - Manages the creation, allocation, and reclamation of ThreadExecutor instances.
 * - Implements dynamic scaling.
 * - Maintains a queue of idle executors and a set of busy executors.
 * - Provides unified thread pool resource management for the entire system.
 *
 * Design Principles:
 * - Global singleton service, managed by a DI (Dependency Injection) container.
 * - All triggered sub-workflows and dynamic threads share the same thread pool instance.
 * - Dynamic scaling: Creates new executors based on load.
 * - Idle executors are reclaimed after a timeout to avoid resource waste.
 *
 * Note: JavaScript uses a single-threaded event loop model, where all state changes are completed within the atomic execution units of the event loop, so no additional locking mechanisms are required. The "threads" mentioned in the project are merely logical concepts (execution instances) and not actual OS threads.
 *
 */

import { ThreadExecutor } from "./executors/thread-executor.js";
import {
  WorkerStatus,
  type ExecutorWrapper,
  type PoolStats,
  type ExecutionPoolConfig,
} from "../../core/types/index.js";
import { now } from "@wf-agent/common-utils";
import { StateManagementError } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ThreadPool" });

/**
 * ThreadPool - Thread Pool Service (Global Singleton)
 */
export class ThreadPool {
  private static instance: ThreadPool | null = null;

  /**
   * All executors
   */
  private allExecutors: Map<string, ExecutorWrapper> = new Map();

  /**
   * Idle Executor Queue
   */
  private idleExecutors: string[] = [];

  /**
   * Busy Executor Set
   */
  private busyExecutors: Set<string> = new Set();

  /**
   * Array of Promises waiting for the executor to execute
   */
  private waitingPromises: Array<{
    resolve: (executor: ThreadExecutor) => void;
    reject: (error: Error) => void;
  }> = [];

  /**
   * ThreadExecutor factory function
   */
  private executorFactory: () => ThreadExecutor;

  /**
   * Configuration
   */
  private config: Required<ExecutionPoolConfig>;

  /**
   * Is it already turned off?
   */
  private isShutdown: boolean = false;

  /**
   * Private constructor to prevent direct instantiation
   */
  private constructor(executorFactory: () => ThreadExecutor, config?: ExecutionPoolConfig) {
    this.executorFactory = executorFactory;
    this.config = {
      minExecutors: config?.minExecutors || 1,
      maxExecutors: config?.maxExecutors || 10,
      idleTimeout: config?.idleTimeout || 30000,
      defaultTimeout: config?.defaultTimeout || 30000,
    };

    this.initializeMinExecutors();
  }

  /**
   * Get a singleton instance
   * @param executorFactory: The ThreadExecutor factory function
   * @param config: The configuration
   * @returns: The singleton instance
   */
  static getInstance(
    executorFactory: () => ThreadExecutor,
    config?: ExecutionPoolConfig,
  ): ThreadPool {
    if (!ThreadPool.instance) {
      ThreadPool.instance = new ThreadPool(executorFactory, config);
    }
    return ThreadPool.instance;
  }

  /**
   * Reset the singleton instance (for testing purposes)
   */
  static resetInstance(): void {
    if (ThreadPool.instance) {
      ThreadPool.instance.shutdown();
      ThreadPool.instance = null;
    }
  }

  /**
   * Initialize the minimum number of executors.
   */
  private initializeMinExecutors(): void {
    for (let i = 0; i < this.config.minExecutors; i++) {
      const executor = this.createExecutor();
      this.allExecutors.set(executor.executorId, executor);
      this.idleExecutors.push(executor.executorId);
    }
  }

  /**
   * Create a new executor
   * @returns Executor wrapper
   */
  private createExecutor(): ExecutorWrapper {
    const executorId = `executor-${now()}-${Math.random().toString(36).substr(2, 9)}`;

    const executor = this.executorFactory();

    const wrapper: ExecutorWrapper = {
      executorId,
      executor,
      status: WorkerStatus.IDLE,
      lastUsedTime: now(),
    };

    return wrapper;
  }

  /**
   * Assign an executor
   *
   * The JavaScript event loop ensures serial execution, eliminating the need for lock protection.
   *
   * @returns Executor instance
   */
  async allocateExecutor(): Promise<ThreadExecutor> {
    if (this.isShutdown) {
      throw new StateManagementError("ThreadPool is shutdown", "threadPool", "read");
    }

    if (this.idleExecutors.length > 0) {
      const executorId = this.idleExecutors.shift()!;
      const wrapper = this.allExecutors.get(executorId)!;

      if (wrapper.idleTimer) {
        clearTimeout(wrapper.idleTimer);
        wrapper.idleTimer = undefined;
      }

      wrapper.status = WorkerStatus.BUSY;
      wrapper.lastUsedTime = now();
      this.busyExecutors.add(executorId);

      return wrapper.executor as ThreadExecutor;
    }

    if (this.allExecutors.size < this.config.maxExecutors) {
      const wrapper = this.createExecutor();
      this.allExecutors.set(wrapper.executorId, wrapper);

      wrapper.status = WorkerStatus.BUSY;
      wrapper.lastUsedTime = now();
      this.busyExecutors.add(wrapper.executorId);

      return wrapper.executor as ThreadExecutor;
    }

    return new Promise<ThreadExecutor>((resolve, reject) => {
      this.waitingPromises.push({ resolve, reject });
    });
  }

  /**
   * Release the executor
   *
   * The JavaScript event loop ensures serial execution, so no locking protection is required.
   *
   * @param executor: Executor instance
   */
  async releaseExecutor(executor: ThreadExecutor): Promise<void> {
    if (this.isShutdown) {
      return;
    }

    let executorId: string | undefined;
    for (const [id, wrapper] of this.allExecutors.entries()) {
      if (wrapper.executor === executor) {
        executorId = id;
        break;
      }
    }

    if (!executorId) {
      logger.warn("Executor not found in pool");
      return;
    }

    const wrapper = this.allExecutors.get(executorId)!;

    this.busyExecutors.delete(executorId);

    if (this.waitingPromises.length > 0) {
      const waiting = this.waitingPromises.shift()!;
      wrapper.status = WorkerStatus.BUSY;
      wrapper.lastUsedTime = now();
      this.busyExecutors.add(executorId);
      waiting.resolve(wrapper.executor as ThreadExecutor);
      return;
    }

    wrapper.status = WorkerStatus.IDLE;
    wrapper.lastUsedTime = now();
    this.idleExecutors.push(executorId);

    this.scheduleIdleTimeout(executorId);
  }

  /**
   * Arrange idle timeout
   * @param executorId Executor ID
   */
  private scheduleIdleTimeout(executorId: string): void {
    const wrapper = this.allExecutors.get(executorId);
    if (!wrapper) {
      return;
    }

    if (this.allExecutors.size > this.config.minExecutors) {
      wrapper.idleTimer = setTimeout(() => {
        this.destroyExecutor(executorId);
      }, this.config.idleTimeout);
    }
  }

  /**
   * Terminate the executor
   * @param executorId: Executor ID
   */
  private async destroyExecutor(executorId: string): Promise<void> {
    const wrapper = this.allExecutors.get(executorId);
    if (!wrapper) {
      return;
    }

    if (wrapper.idleTimer) {
      clearTimeout(wrapper.idleTimer);
    }

    const index = this.idleExecutors.indexOf(executorId);
    if (index > -1) {
      this.idleExecutors.splice(index, 1);
    }

    this.allExecutors.delete(executorId);

    const executorWithCleanup = wrapper.executor as { cleanup?: () => void };
    if (typeof executorWithCleanup.cleanup === "function") {
      executorWithCleanup.cleanup();
    }
  }

  /**
   * Get statistical information
   * @returns Statistical information
   */
  getStats(): PoolStats {
    return {
      totalExecutors: this.allExecutors.size,
      idleExecutors: this.idleExecutors.length,
      busyExecutors: this.busyExecutors.size,
      minExecutors: this.config.minExecutors,
      maxExecutors: this.config.maxExecutors,
    };
  }

  /**
   * Get the configuration
   * @returns Configuration
   */
  getConfig(): Required<ExecutionPoolConfig> {
    return this.config;
  }

  /**
   * Close the thread pool.
   */
  async shutdown(): Promise<void> {
    if (this.isShutdown) {
      return;
    }

    this.isShutdown = true;

    for (const waiting of this.waitingPromises) {
      waiting.reject(new StateManagementError("ThreadPool is shutdown", "threadPool", "read"));
    }
    this.waitingPromises = [];

    while (this.busyExecutors.size > 0) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }

    for (const executorId of this.idleExecutors) {
      await this.destroyExecutor(executorId);
    }

    this.allExecutors.clear();
    this.idleExecutors = [];
    this.busyExecutors.clear();
  }

  /**
   * Check if it is closed.
   * @returns Whether it is closed.
   */
  isShutdownFlag(): boolean {
    return this.isShutdown;
  }
}
