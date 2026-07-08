/**
 * UnifiedTaskScheduler - Centralized Task Scheduling
 *
 * Responsibilities:
 * - Unified scheduling policy for all task sources (ExecutionQueue, TriggeredExecutionManager, etc.)
 * - Priority-based execution (FIFO with configurable priority levels)
 * - Global concurrency limiting
 * - Resource-aware scheduling
 * - Timeout management with recovery support
 * - Fair scheduling across different execution sources
 *
 * Design Principles:
 * - Single source of truth for task scheduling decisions
 * - Decouples task submission from task execution
 * - Supports multiple priority levels
 * - Tracks execution deadlines for recovery after system restart
 * - Non-blocking, event-driven architecture
 */

import { now } from "@wf-agent/common-utils";
import type { TimeoutPolicy } from "../types/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "UnifiedTaskScheduler" });

/**
 * Task priority levels (higher number = higher priority)
 * DEFAULT(0) < LOW(1) < NORMAL(2) < HIGH(3) < CRITICAL(4)
 */
export type TaskPriority = 0 | 1 | 2 | 3 | 4;

export const TaskPriorityLevel = {
  DEFAULT: 0,
  LOW: 1,
  NORMAL: 2,
  HIGH: 3,
  CRITICAL: 4,
} as const;

/**
 * Scheduled task entry for internal tracking
 */
interface ScheduledTaskEntry {
  taskId: string;
  sourceId: string;                    // Which executor/manager submitted this
  priority: TaskPriority;
  submitTime: number;
  timeout?: number;
  deadlineTime?: number;               // Absolute deadline
  timeoutPolicy: TimeoutPolicy;
  callbacks: {
    onReady: () => Promise<void>;
    onTimeout: () => Promise<void>;
  };
}

/**
 * Scheduler Configuration
 */
export interface TaskSchedulerConfig {
  /** Maximum number of tasks executing concurrently (default: CPU cores) */
  maxConcurrentTasks?: number;
  /** Enable auto-recovery of expired tasks (default: true) */
  enableTimeoutRecovery?: boolean;
  /** Timeout recovery check interval (default: 5000ms) */
  timeoutCheckInterval?: number;
  /** Fair scheduling mode - ensure all sources get time (default: true) */
  fairScheduling?: boolean;
}

/**
 * UnifiedTaskScheduler - Manages task scheduling across all execution sources
 */
export class UnifiedTaskScheduler {
  /**
   * Pending task queue (priority-based)
   * Organized as Map<priority, taskId[]> for efficient priority handling
   */
  private pendingByPriority: Map<TaskPriority, string[]> = new Map();

  /**
   * All scheduled tasks (for lookup by taskId)
   */
  private tasks: Map<string, ScheduledTaskEntry> = new Map();

  /**
   * Currently executing task IDs
   */
  private executingTasks: Set<string> = new Set();

  /**
   * Source fair scheduling state (round-robin position for each source)
   */
  private sourceRoundRobinIndex: Map<string, number> = new Map();

  /**
   * Configuration
   */
  private config: Required<TaskSchedulerConfig>;

  /**
   * Timeout recovery guard
   */
  private timeoutRecoveryGuard?: NodeJS.Timeout;

  /**
   * Is scheduler active?
   */
  private isActive: boolean = false;

  constructor(config?: TaskSchedulerConfig) {
    this.config = {
      maxConcurrentTasks: config?.maxConcurrentTasks || (this.getDefaultMaxConcurrent()),
      enableTimeoutRecovery: config?.enableTimeoutRecovery ?? true,
      timeoutCheckInterval: config?.timeoutCheckInterval ?? 5000,
      fairScheduling: config?.fairScheduling ?? true,
    };

    // Initialize priority buckets
    for (const level of Object.values(TaskPriorityLevel)) {
      this.pendingByPriority.set(level as TaskPriority, []);
    }
  }

  /**
   * Start the scheduler
   */
  async start(): Promise<void> {
    if (this.isActive) return;

    this.isActive = true;
    logger.info("UnifiedTaskScheduler started", {
      maxConcurrentTasks: this.config.maxConcurrentTasks,
      fairScheduling: this.config.fairScheduling,
    });

    // Start timeout recovery if enabled
    if (this.config.enableTimeoutRecovery) {
      this.startTimeoutRecovery();
    }
  }

  /**
   * Stop the scheduler
   */
  async stop(): Promise<void> {
    this.isActive = false;
    if (this.timeoutRecoveryGuard) {
      clearInterval(this.timeoutRecoveryGuard);
    }
    logger.info("UnifiedTaskScheduler stopped");
  }

  /**
   * Schedule a task for execution
   * Tasks are queued by priority and executed when resources become available
   *
   * @param taskId Task ID
   * @param sourceId Source identifier (executor/manager name)
   * @param timeout Timeout in milliseconds
   * @param callbacks Execution callbacks
   * @param priority Task priority (default: NORMAL)
   * @param timeoutPolicy Timeout policy (default: 'cancel')
   */
  scheduleTask(
    taskId: string,
    sourceId: string,
    timeout: number | undefined,
    callbacks: { onReady: () => Promise<void>; onTimeout: () => Promise<void> },
    priority: TaskPriority = TaskPriorityLevel.NORMAL,
    timeoutPolicy: TimeoutPolicy = 'cancel',
  ): void {
    const submitTime = now();
    const deadlineTime = timeout ? submitTime + timeout : undefined;

    const entry: ScheduledTaskEntry = {
      taskId,
      sourceId,
      priority,
      submitTime,
      timeout,
      deadlineTime,
      timeoutPolicy,
      callbacks,
    };

    this.tasks.set(taskId, entry);

    // Add to priority queue
    const bucket = this.pendingByPriority.get(priority) || [];
    bucket.push(taskId);
    this.pendingByPriority.set(priority, bucket);

    logger.debug("Task scheduled", {
      taskId,
      priority,
      sourceId,
      deadline: deadlineTime,
    });

    // Try to dispatch tasks
    this.dispatchPending();
  }

  /**
   * Dispatch pending tasks (FIFO within priority, round-robin across sources)
   * Respects max concurrent limit
   */
  private async dispatchPending(): Promise<void> {
    while (this.executingTasks.size < this.config.maxConcurrentTasks && this.hasPending()) {
      const taskId = this.config.fairScheduling
        ? this.getNextTaskFairly()
        : this.getNextTaskByPriority();

      if (!taskId) break;

      const entry = this.tasks.get(taskId);
      if (!entry) continue;

      this.executingTasks.add(taskId);
      this.pendingByPriority.get(entry.priority)?.shift();

      // Execute task asynchronously (non-blocking)
      this.executeScheduledTask(entry)
        .catch(error => {
          logger.error("Task execution failed", { taskId, error });
        })
        .finally(() => {
          this.executingTasks.delete(taskId);
          // Continue dispatching
          this.dispatchPending();
        });
    }
  }

  /**
   * Execute a scheduled task with timeout guard
   */
  private async executeScheduledTask(entry: ScheduledTaskEntry): Promise<void> {
    let timeoutGuard: NodeJS.Timeout | undefined;

    try {
      // Set up timeout guard
      if (entry.timeout && entry.timeout > 0) {
        timeoutGuard = setTimeout(() => {
          logger.warn("Task timeout", {
            taskId: entry.taskId,
            timeout: entry.timeout,
            policy: entry.timeoutPolicy,
          });
          entry.callbacks.onTimeout();
        }, entry.timeout);
      }

      // Execute task
      await entry.callbacks.onReady();
    } finally {
      if (timeoutGuard) {
        clearTimeout(timeoutGuard);
      }
      this.tasks.delete(entry.taskId);
    }
  }

  /**
   * Get next task by priority (FIFO within priority)
   */
  private getNextTaskByPriority(): string | null {
    // Try priorities from highest to lowest
    for (let p = TaskPriorityLevel.CRITICAL; p >= TaskPriorityLevel.DEFAULT; p--) {
      const bucket = this.pendingByPriority.get(p as TaskPriority);
      if (bucket && bucket.length > 0) {
        const taskId = bucket[0];
        if (taskId) {
          return taskId;  // Don't remove yet, done in dispatchPending
        }
      }
    }
    return null;
  }

  /**
   * Get next task with fair scheduling (round-robin across sources)
   * Ensures all task sources get execution time regardless of priority
   */
  private getNextTaskFairly(): string | null {
    // Collect all pending tasks grouped by source
    const tasksBySource = new Map<string, string[]>();

    for (const taskId of this.tasks.keys()) {
      const entry = this.tasks.get(taskId);
      if (!entry) continue;

      const isInQueue = Array.from(this.pendingByPriority.values()).some(
        bucket => bucket.includes(taskId)
      );
      if (!isInQueue) continue;

      const sourceId = entry.sourceId;
      if (!tasksBySource.has(sourceId)) {
        tasksBySource.set(sourceId, []);
      }
      tasksBySource.get(sourceId)!.push(taskId);
    }

    if (tasksBySource.size === 0) return null;

    // Round-robin across sources
    const sources = Array.from(tasksBySource.keys());
    const globalRRIdx = this.sourceRoundRobinIndex.get('__global__') ?? 0;

    for (let i = 0; i < sources.length; i++) {
      const sourceIdx = (globalRRIdx + i) % sources.length;
      const source = sources[sourceIdx];
      if (!source) continue;  // Defensive check

      const tasks = tasksBySource.get(source);

      if (tasks && tasks.length > 0) {
        const taskId = tasks[0];
        if (taskId) {
          // Update round-robin position for next time
          this.sourceRoundRobinIndex.set('__global__', sourceIdx + 1);
          return taskId;
        }
      }
    }

    return null;
  }

  /**
   * Check if there are pending tasks
   */
  private hasPending(): boolean {
    return Array.from(this.pendingByPriority.values()).some(bucket => bucket.length > 0);
  }

  /**
   * Start timeout recovery (check for expired tasks)
   */
  private startTimeoutRecovery(): void {
    this.timeoutRecoveryGuard = setInterval(() => {
      this.checkExpiredTasks();
    }, this.config.timeoutCheckInterval);
  }

  /**
   * Check for expired tasks and apply recovery policy
   */
  private checkExpiredTasks(): void {
    const currentTime = now();

    for (const [taskId, entry] of this.tasks.entries()) {
      if (!entry.deadlineTime || entry.deadlineTime >= currentTime) {
        continue;  // Not expired
      }

      // Task has expired
      if (this.executingTasks.has(taskId)) {
        continue;  // Already executing, timeout guard will handle it
      }

      logger.warn("Expired task found in queue", {
        taskId,
        deadlineTime: entry.deadlineTime,
        policy: entry.timeoutPolicy,
      });

      switch (entry.timeoutPolicy) {
        case 'cancel':
          // Silently cancel - will be removed when executor processes it
          this.removeFromQueue(taskId);
          break;

        case 'escalate':
          // Log for manual review
          logger.error("Task requires manual escalation", {
            taskId,
            deadlineTime: entry.deadlineTime,
          });
          break;

        case 'manual':
          // Leave in queue for manual intervention
          logger.warn("Task awaiting manual intervention", { taskId });
          break;
      }
    }
  }

  /**
   * Remove task from queue
   */
  private removeFromQueue(taskId: string): void {
    for (const bucket of this.pendingByPriority.values()) {
      const idx = bucket.indexOf(taskId);
      if (idx > -1) {
        bucket.splice(idx, 1);
        break;
      }
    }
  }

  /**
   * Cancel a scheduled task
   * Returns true if task was cancelled (was in queue), false if already executing
   */
  cancelTask(taskId: string): boolean {
    if (this.executingTasks.has(taskId)) {
      return false;  // Already executing, cannot cancel from scheduler
    }

    this.removeFromQueue(taskId);
    this.tasks.delete(taskId);
    logger.debug("Task cancelled", { taskId });
    return true;
  }

  /**
   * Get task info
   */
  getTaskInfo(taskId: string): ScheduledTaskEntry | null {
    return this.tasks.get(taskId) || null;
  }

  /**
   * Get scheduler statistics
   */
  getStats() {
    return {
      pending: Array.from(this.pendingByPriority.values()).reduce((sum, b) => sum + b.length, 0),
      executing: this.executingTasks.size,
      total: this.tasks.size,
      maxConcurrent: this.config.maxConcurrentTasks,
    };
  }

  /**
   * Default max concurrent tasks (CPU cores)
   */
  private getDefaultMaxConcurrent(): number {
    try {
      const os = require("os");
      return Math.max(2, os.cpus().length - 1);
    } catch {
      return 4;  // Fallback
    }
  }
}
