/**
 * AgentExecutionContext - Agent Execution Manager
 *
 * Responsibilities:
 * - Manages the execution of AgentLoopEntity instances
 * - Provides unified interface for sync and async execution
 * - Coordinates with ExecutionPool and ExecutionQueue
 * - Handles task cancellation and status tracking
 *
 * Design Principles:
 * - Unified execution management for Agent instances
 * - Uses shared Pool/Queue infrastructure
 * - Independent configuration from workflow execution
 * - Fault isolation from Graph execution
 */

import { TaskRegistry, type TaskManager } from "../../workflow/stores/task/task-registry.js";
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
import type { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import type { AgentLoopResult } from "@wf-agent/types";
import type { ExecutionPoolConfig, ExecutionInstanceType } from "../../core/types/index.js";

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
 * AgentExecutionContext dependencies
 */
export type AgentExecutionManagerDependencies = BaseExecutionManagerDependencies<
  AgentLoopEntity,
  AgentLoopResult
>;

/**
 * Default Agent pool configuration
 */
const DEFAULT_AGENT_POOL_CONFIG: ExecutionPoolConfig = {
  minExecutors: 1,
  maxExecutors: 5,
  idleTimeout: 60000,
  defaultTimeout: 120000,
};

/**
 * Default pool ID for agent execution
 */
const DEFAULT_AGENT_POOL_ID = "agent-execution-pool";

/**
 * AgentExecutionContext - Agent Execution Manager
 *
 * Provides unified execution management for AgentLoopEntity instances.
 * Uses shared Pool/Queue infrastructure for resource management.
 */
export class AgentExecutionContext implements TaskManager {
  /** Task registry */
  private taskRegistry: TaskRegistry;

  /** Pool service */
  private poolService: ExecutionPool<AgentLoopEntity>;

  /** Queue manager */
  private queueManager: ExecutionQueue<AgentLoopEntity>;

  /** Event manager */
  private eventManager: EventRegistry;

  /** Execute function */
  private executeFn: (
    executor: Executor<AgentLoopEntity>,
    instance: AgentLoopEntity,
  ) => Promise<AgentLoopResult>;

  /** Instance type */
  private instanceType: ExecutionInstanceType;

  /**
   * Constructor
   * @param deps Dependencies
   */
  constructor(deps: AgentExecutionManagerDependencies) {
    this.taskRegistry = TaskRegistry.getInstance();
    this.eventManager = deps.eventManager;
    this.executeFn = deps.executeFn;
    this.instanceType = deps.instanceType ?? "agent";

    const poolConfig = deps.poolConfig ?? DEFAULT_AGENT_POOL_CONFIG;
    const poolId = deps.poolId ?? DEFAULT_AGENT_POOL_ID;

    // Create pool service with configurable pool ID
    this.poolService = ExecutionPool.getInstance<AgentLoopEntity>(
      poolId,
      deps.executorFactory,
      poolConfig,
    );

    // Create queue manager
    this.queueManager = new ExecutionQueue<AgentLoopEntity>(
      this.taskRegistry,
      this.poolService,
      this.eventManager,
      async (executor: Executor<AgentLoopEntity>, instance: AgentLoopEntity) => {
        return this.executeFn(executor, instance);
      },
    );
  }

  /**
   * Execute Agent synchronously
   * @param agentEntity Agent entity to execute
   * @param timeout Optional timeout in milliseconds
   * @returns Execution result
   */
  async executeSync(
    agentEntity: AgentLoopEntity,
    timeout?: number,
  ): Promise<TypedExecutionResult<AgentLoopResult>> {
    // Register task
    const taskId = this.taskRegistry.register(agentEntity, this.instanceType, this, timeout);

    // Submit to queue
    const result = await this.queueManager.submitSync(
      taskId,
      agentEntity,
      this.instanceType,
      timeout,
    );

    return {
      ...result,
      result: result.agentResult as AgentLoopResult,
    };
  }

  /**
   * Execute Agent asynchronously
   * @param agentEntity Agent entity to execute
   * @param timeout Optional timeout in milliseconds
   * @returns Task submission result
   */
  executeAsync(agentEntity: AgentLoopEntity, timeout?: number): TaskSubmissionResult {
    // Register task
    const taskId = this.taskRegistry.register(agentEntity, this.instanceType, this, timeout);

    // Submit to queue
    return this.queueManager.submitAsync(taskId, agentEntity, this.instanceType, timeout);
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
