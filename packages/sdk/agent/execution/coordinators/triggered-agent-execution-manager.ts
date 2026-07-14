/**
 * TriggeredAgentExecutionManager - Triggered Agent Execution Manager (Simplified)
 *
 * Manages the execution of triggered (nested) agent loop executions.
 * Mirrors the design of TriggeredWorkflowExecutionManager for symmetry.
 *
 * Design Principles:
 * - Single responsibility: manage triggered agent loop executions
 * - Simplified pending queue (FIFO, no complex state tracking)
 * - Direct integration with TaskRegistry (single source of truth for state)
 * - Delegates executor handling to coordinator callback
 * - Mirrors: TriggeredWorkflowExecutionManager (symmetric design pattern)
 */

import type { AgentLoopRuntimeConfig, AgentLoopResult } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { TaskSubmissionResult } from "../../../workflow/execution/types/triggered-subworkflow.types.js";
import { TaskRegistry, type TaskManager } from "../../../shared/registry/task-registry.js";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TriggeredAgentExecutionManager" });

/**
 * Executor callback for executing agent loops
 * Called to execute a pending agent loop
 */
export type AgentExecutorCallback = (
  entity: AgentLoopEntity,
  config: AgentLoopRuntimeConfig,
) => Promise<AgentLoopResult>;

/**
 * Triggered Execution Configuration
 */
export interface TriggeredAgentExecutionConfig {
  /** Execution ID of the triggered execution */
  executionId: string;

  /** Parent execution entity (workflow) */
  parentEntity: any;

  /** Type of parent (currently WORKFLOW, future: AGENT_LOOP) */
  parentType: "WORKFLOW" | "AGENT_LOOP";

  /** Timeout in milliseconds */
  timeout?: number;

  /** Whether to wait for completion (sync vs async) */
  waitForCompletion?: boolean;
}

/**
 * Simple pending task info
 */
interface PendingTaskInfo {
  taskId: string;
  entity: AgentLoopEntity;
  config: AgentLoopRuntimeConfig;
  isSync: boolean;
  resolve?: (value: AgentLoopResult) => void;
  reject?: (reason: Error) => void;
}

/**
 * TriggeredAgentExecutionManager - Simplified implementation
 *
 * Manages triggered agent loop executions with the same pattern as TriggeredWorkflowExecutionManager.
 * Supports both sync (wait for completion) and async (return immediately) execution modes.
 */
export class TriggeredAgentExecutionManager implements TaskManager {
  private taskRegistry: TaskRegistry;
  private executorCallback: AgentExecutorCallback;

  // Simplified pending queue: just store task IDs in FIFO order
  private pendingTaskIds: string[] = [];
  private taskInfoMap: Map<string, PendingTaskInfo> = new Map();

  // Track which tasks are currently executing
  private executingTaskIds: Set<string> = new Set();

  constructor(
    taskRegistry: TaskRegistry,
    executorCallback: AgentExecutorCallback,
  ) {
    this.taskRegistry = taskRegistry;
    this.executorCallback = executorCallback;
  }

  /**
   * Submit a triggered agent execution
   * Routes to sync or async based on configuration
   *
   * @param config Triggered execution configuration
   * @param entity The agent loop entity
   * @param agentConfig Agent loop runtime configuration
   * @returns Execution result (sync) or task submission result (async)
   */
  async submitTriggeredExecution(
    config: TriggeredAgentExecutionConfig,
    entity: AgentLoopEntity,
    agentConfig: AgentLoopRuntimeConfig,
  ): Promise<AgentLoopResult | TaskSubmissionResult> {
    const executionId = config.executionId;
    const waitForCompletion = config.waitForCompletion !== false;
    const timeout = config.timeout || 30000; // Default timeout

    // Register task in registry
    const taskId = this.taskRegistry.register(entity, "agent", this, timeout);

    logger.debug("Triggered agent execution registered", {
      executionId,
      taskId,
      waitForCompletion,
    });

    if (waitForCompletion) {
      // Sync execution: wait for completion
      return new Promise((resolve, reject) => {
        this.pendingTaskIds.push(taskId);
        this.taskInfoMap.set(taskId, {
          taskId,
          entity,
          config: agentConfig,
          isSync: true,
          resolve,
          reject,
        });

        // Trigger processing (non-blocking)
        this.processPendingTasks();
      });
    } else {
      // Async execution: submit and return immediately
      this.pendingTaskIds.push(taskId);
      this.taskInfoMap.set(taskId, {
        taskId,
        entity,
        config: agentConfig,
        isSync: false,
      });

      // Trigger processing (non-blocking)
      this.processPendingTasks();

      return {
        taskId,
        status: "QUEUED",
        message: "Triggered agent execution submitted",
        submitTime: now(),
      };
    }
  }

  /**
   * Process pending tasks sequentially
   * Non-blocking: can be called multiple times safely
   */
  private processingQueue = false;

  private async processPendingTasks(): Promise<void> {
    // Simple guard: only one processor at a time
    if (this.processingQueue) {
      return;
    }
    this.processingQueue = true;

    try {
      while (this.pendingTaskIds.length > 0) {
        const taskId = this.pendingTaskIds[0]!;
        const taskInfo = this.taskInfoMap.get(taskId);

        if (!taskInfo) {
          // Task was cancelled
          this.pendingTaskIds.shift();
          continue;
        }

        // Double-check task still exists (might have been cancelled)
        if (!this.taskInfoMap.has(taskId)) {
          this.pendingTaskIds.shift();
          continue;
        }

        // Remove from pending queue
        this.pendingTaskIds.shift();

        // Mark as executing
        this.executingTaskIds.add(taskId);

        // Update status to RUNNING
        this.taskRegistry.updateStatus(taskId, "RUNNING");

        // Execute asynchronously (don't await here, let it run in background)
        this.executeTask(taskId, taskInfo);
      }
    } finally {
      this.processingQueue = false;
    }
  }

  /**
   * Execute a single task
   * This runs in the background for async tasks, or resolves the promise for sync tasks
   */
  private async executeTask(taskId: string, taskInfo: PendingTaskInfo): Promise<void> {
    const startTime = now();

    try {
      // Execute agent loop via callback
      const result = await this.executorCallback(taskInfo.entity, taskInfo.config);

      const executionTime = now() - startTime;

      // Update status
      this.taskRegistry.updateStatus(taskId, "COMPLETED", { result });

      // For sync tasks, resolve the promise
      if (taskInfo.isSync && taskInfo.resolve) {
        taskInfo.resolve(result);
      }

      logger.debug("Triggered agent execution completed", {
        taskId,
        executionTime,
        executionId: taskInfo.entity.id,
        success: result.success,
      });
    } catch (error) {
      const executionTime = now() - startTime;
      const errorObj = error as Error;

      // Update status
      this.taskRegistry.updateStatus(taskId, "FAILED", { error: errorObj });

      // For sync tasks, reject the promise
      if (taskInfo.isSync && taskInfo.reject) {
        taskInfo.reject(errorObj);
      }

      logger.error("Triggered agent execution failed", {
        taskId,
        executionTime,
        executionId: taskInfo.entity.id,
        error: errorObj.message,
      });
    } finally {
      // Remove from executing set
      this.executingTaskIds.delete(taskId);

      // Remove from task info map
      this.taskInfoMap.delete(taskId);

      // Continue processing next tasks
      this.processPendingTasks();
    }
  }

  /**
   * Cancel a task (implements TaskManager interface)
   * @param taskId Task ID
   * @returns Whether cancellation was successful
   */
  async cancelTask(taskId: string): Promise<boolean> {
    // Can only cancel pending tasks (not executing)
    const pendingIndex = this.pendingTaskIds.indexOf(taskId);
    if (pendingIndex > -1) {
      this.pendingTaskIds.splice(pendingIndex, 1);
      this.taskInfoMap.delete(taskId);
      this.taskRegistry.updateStatus(taskId, "CANCELLED");

      logger.debug("Pending agent task cancelled", { taskId });
      return true;
    }

    // Running tasks cannot be cancelled
    if (this.executingTaskIds.has(taskId)) {
      logger.warn("Cannot cancel executing agent task", { taskId });
      return false;
    }

    return false;
  }

  /**
   * Get task status (implements TaskManager interface)
   * @param taskId Task ID
   * @returns Task information or null
   */
  getTaskStatus(taskId: string) {
    return this.taskRegistry.get(taskId);
  }

  /**
   * Get queue statistics
   */
  getQueueStats() {
    const stats = this.taskRegistry.getStats();
    return {
      pendingCount: this.pendingTaskIds.length,
      runningCount: this.executingTaskIds.size,
      completedCount: stats.completed,
      failedCount: stats.failed,
      cancelledCount: stats.cancelled,
    };
  }

  /**
   * Get task registry statistics
   */
  getTaskStats() {
    return this.taskRegistry.getStats();
  }
}
