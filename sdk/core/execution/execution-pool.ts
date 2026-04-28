/**
 * ExecutionPool - Generic Execution Pool
 *
 * Responsibilities:
 * - Manages the creation, allocation, and reclamation of Executor instances.
 * - Implements dynamic scaling.
 * - Maintains a queue of idle executors and a set of busy executors.
 * - Provides unified execution pool resource management for both Agent and Thread.
 *
 * Design Principles:
 * - Generic design supporting any executor type.
 * - Dynamic scaling: Creates new executors based on load.
 * - Idle executors are reclaimed after a timeout to avoid resource waste.
 */

import {
  WorkerStatus,
  type ExecutorWrapper,
  type PoolStats,
  type ExecutionPoolConfig,
} from "../types/index.js";
import { now } from "@wf-agent/common-utils";
import { StateManagementError } from "@wf-agent/types";
import { sdkLogger as logger } from "../../utils/logger.js";

/**
 * Executor interface - generic executor contract
 */
export interface Executor<T> {
  /** Executor identifier */
  id?: string;
  /** Execute the instance */
  execute(instance: T): Promise<unknown>;
  /** Cleanup resources */
  cleanup?(): void;
}

/**
 * Executor factory interface
 */
export interface ExecutorFactory<T> {
  /** Create a new executor instance */
  create(): Executor<T>;
}

/**
 * ExecutionPool - Generic Execution Pool
 *
 * @template T - The execution instance type (e.g., ThreadEntity, AgentLoopEntity)
 */
export class ExecutionPool<T> {
  private static instances: Map<string, ExecutionPool<unknown>> = new Map();

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
    resolve: (executor: Executor<T>) => void;
    reject: (error: Error) => void;
  }> = [];

  /**
   * Executor factory
   */
  private executorFactory: ExecutorFactory<T>;

  /**
   * Configuration
   */
  private config: Required<ExecutionPoolConfig>;

  /**
   * Is it already turned off?
   */
  private isShutdown: boolean = false;

  /**
   * Pool identifier
   */
  private poolId: string;

  /**
   * Private constructor to prevent direct instantiation
   */
  private constructor(
    poolId: string,
    executorFactory: ExecutorFactory<T>,
    config?: ExecutionPoolConfig,
  ) {
    this.poolId = poolId;
    this.executorFactory = executorFactory;
    this.config = {
      minExecutors: config?.minExecutors || 1,
      maxExecutors: config?.maxExecutors || 10,
      idleTimeout: config?.idleTimeout || 30000,
      defaultTimeout: config?.defaultTimeout || 30000,
    };

    // Initialize the minimum number of executors.
    this.initializeMinExecutors();
  }

  /**
   * Get a singleton instance for a specific pool
   * @param poolId Pool identifier
   * @param executorFactory The executor factory function
   * @param config The configuration
   * @returns The singleton instance
   */
  static getInstance<T>(
    poolId: string,
    executorFactory: ExecutorFactory<T>,
    config?: ExecutionPoolConfig,
  ): ExecutionPool<T> {
    if (!ExecutionPool.instances.has(poolId)) {
      ExecutionPool.instances.set(poolId, new ExecutionPool(poolId, executorFactory, config));
    }
    return ExecutionPool.instances.get(poolId)!;
  }

  /**
   * Reset a specific pool instance (for testing purposes)
   * @param poolId Pool identifier
   */
  static resetInstance(poolId: string): void {
    const instance = ExecutionPool.instances.get(poolId);
    if (instance) {
      instance.shutdown();
      ExecutionPool.instances.delete(poolId);
    }
  }

  /**
   * Reset all pool instances (for testing purposes)
   */
  static resetAllInstances(): void {
    for (const [poolId] of ExecutionPool.instances) {
      ExecutionPool.resetInstance(poolId);
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
    const executorId = `executor-${this.poolId}-${now()}-${Math.random().toString(36).substr(2, 9)}`;

    // Create an executor using a factory
    const executor = this.executorFactory.create();

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
  async allocateExecutor(): Promise<Executor<T>> {
    if (this.isShutdown) {
      throw new StateManagementError(
        `ExecutionPool (${this.poolId}) is shutdown`,
        "executionPool",
        "read",
      );
    }

    // Check if there are any available executors available for execution.
    if (this.idleExecutors.length > 0) {
      const executorId = this.idleExecutors.shift()!;
      const wrapper = this.allExecutors.get(executorId)!;

      // Clear the idle timeout timer
      if (wrapper.idleTimer) {
        clearTimeout(wrapper.idleTimer);
        wrapper.idleTimer = undefined;
      }

      // Update status
      wrapper.status = WorkerStatus.BUSY;
      wrapper.lastUsedTime = now();
      this.busyExecutors.add(executorId);

      return wrapper.executor as Executor<T>;
    }

    // Check if it is possible to create a new executor.
    if (this.allExecutors.size < this.config.maxExecutors) {
      const wrapper = this.createExecutor();
      this.allExecutors.set(wrapper.executorId, wrapper);

      // Update status
      wrapper.status = WorkerStatus.BUSY;
      wrapper.lastUsedTime = now();
      this.busyExecutors.add(wrapper.executorId);

      return wrapper.executor as Executor<T>;
    }

    // Waiting for an available executor to execute.
    return new Promise((resolve, reject) => {
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
  async releaseExecutor(executor: Executor<T>): Promise<void> {
    if (this.isShutdown) {
      return;
    }

    // Find the executor wrapper.
    let executorId: string | undefined;
    for (const [id, wrapper] of this.allExecutors.entries()) {
      if (wrapper.executor === executor) {
        executorId = id;
        break;
      }
    }

    if (!executorId) {
      logger.warn(`Executor not found in pool ${this.poolId}`, { poolId: this.poolId });
      return;
    }

    const wrapper = this.allExecutors.get(executorId)!;

    // Remove from the busy set.
    this.busyExecutors.delete(executorId);

    // Check if there are any pending Promises.
    if (this.waitingPromises.length > 0) {
      const waiting = this.waitingPromises.shift()!;
      wrapper.status = WorkerStatus.BUSY;
      wrapper.lastUsedTime = now();
      this.busyExecutors.add(executorId);
      waiting.resolve(wrapper.executor as Executor<T>);
      return;
    }

    // Add to the idle queue
    wrapper.status = WorkerStatus.IDLE;
    wrapper.lastUsedTime = now();
    this.idleExecutors.push(executorId);

    // Set an idle timeout timer
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

    // If the number of executors exceeds the minimum value, set a timeout.
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

    // Clear the timer
    if (wrapper.idleTimer) {
      clearTimeout(wrapper.idleTimer);
    }

    // Remove from the idle queue
    const index = this.idleExecutors.indexOf(executorId);
    if (index > -1) {
      this.idleExecutors.splice(index, 1);
    }

    // Remove from all executors.
    this.allExecutors.delete(executorId);

    // Clean up resources (if the executor has a cleanup method)
    const executor = wrapper.executor as Executor<T>;
    if (typeof executor.cleanup === "function") {
      executor.cleanup();
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
   * Get the pool identifier
   * @returns Pool ID
   */
  getPoolId(): string {
    return this.poolId;
  }

  /**
   * Close the execution pool.
   */
  async shutdown(): Promise<void> {
    if (this.isShutdown) {
      return;
    }

    this.isShutdown = true;

    // Reject all pending Promises.
    for (const waiting of this.waitingPromises) {
      waiting.reject(
        new StateManagementError(
          `ExecutionPool (${this.poolId}) is shutdown`,
          "executionPool",
          "read",
        ),
      );
    }
    this.waitingPromises = [];

    // Wait for all busy executors to complete.
    while (this.busyExecutors.size > 0) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }

    // Terminate all idle executors.
    for (const executorId of this.idleExecutors) {
      await this.destroyExecutor(executorId);
    }

    // Clear all executors.
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
