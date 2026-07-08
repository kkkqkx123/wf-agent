/**
 * ExecutionQueue Unit Tests
 *
 * Tests for:
 * - submitSync: synchronous task submission with promise-based result
 * - submitAsync: fire-and-forget submission
 * - processQueue: internal queue processing
 * - cancelTask: cancelling pending tasks
 * - drain: waiting for all tasks to complete
 * - clear: clearing the pending queue
 * - getQueueStats: querying queue statistics
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { ExecutionQueue } from "../execution-queue.js";
import { ExecutionPool, type Executor, type ExecutorFactory } from "../execution-pool.js";
import type { ExecutionInstance } from "../../types/index.js";
import type { TaskRegistry } from "../../../shared/stores/task-registry.js";
import type { EventRegistry } from "../../registry/event-registry.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createMockTaskRegistry(): TaskRegistry {
  return {
    updateStatus: vi.fn(),
    getStats: vi.fn().mockReturnValue({
      completed: 0,
      failed: 0,
      cancelled: 0,
      registered: 0,
    }),
  } as unknown as TaskRegistry;
}

function createMockEventRegistry(): EventRegistry {
  return {
    emit: vi.fn().mockResolvedValue(undefined),
  } as unknown as EventRegistry;
}

/** Create a dedicated pool for each test to avoid cross-test interference */
function createFreshPool(
  factory: ExecutorFactory<ExecutionInstance>,
  config?: { minExecutors?: number; maxExecutors?: number },
): ExecutionPool<ExecutionInstance> {
  const poolId = `qpool-${Math.random().toString(36).substr(2, 9)}`;
  return ExecutionPool.getInstance<ExecutionInstance>(poolId, factory, {
    minExecutors: config?.minExecutors ?? 1,
    maxExecutors: config?.maxExecutors ?? 5,
    idleTimeout: 30_000,
    defaultTimeout: 30_000,
  });
}

function createWFInstance(id: string): ExecutionInstance {
  return {
    id,
    instanceType: "workflowExecution" as const,
    getExecutionId: () => id,
    getWorkflowId: () => "wf-1",
    getTriggeredSubworkflowId: () => "sub-1",
    getOutput: () => ({}),
  } as unknown as ExecutionInstance;
}

function createAgentInstance(id: string): ExecutionInstance {
  return {
    id,
    instanceType: "agent" as const,
    config: {},
    conversationManager: {},
  } as unknown as ExecutionInstance;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("ExecutionQueue", () => {
  let taskRegistry: TaskRegistry;
  let eventRegistry: EventRegistry;
  let executeFn: ReturnType<typeof vi.fn>;
  let executorFactory: ExecutorFactory<ExecutionInstance>;

  beforeEach(() => {
    taskRegistry = createMockTaskRegistry();
    eventRegistry = createMockEventRegistry();
    executeFn = vi
      .fn<
        (executor: Executor<ExecutionInstance>, instance: ExecutionInstance) => Promise<unknown>
      >()
      .mockImplementation(
        async (_executor: Executor<ExecutionInstance>, instance: ExecutionInstance) => {
          return { status: "completed", executionId: instance.id };
        },
      );
    executorFactory = {
      create(): Executor<ExecutionInstance> {
        return {
          id: `exec-${Math.random().toString(36).substr(2, 6)}`,
          async execute(instance: ExecutionInstance) {
            return { status: "completed", executionId: instance.id };
          },
        };
      },
    };
    ExecutionPool.resetAllInstances();
  });

  afterEach(() => {
    ExecutionPool.resetAllInstances();
  });

  // -----------------------------------------------------------------------
  // submitSync
  // -----------------------------------------------------------------------

  describe("submitSync", () => {
    it("should execute the task and return the result", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("task-1");
      const result = await queue.submitSync("task-1", instance, "workflowExecution");

      expect(result.instance).toBe(instance);
      expect(result.executionResult).toBeDefined();
      expect(typeof result.executionTime).toBe("number");
    });

    it("should call taskRegistry.updateStatusToRunning", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("task-run-1");
      await queue.submitSync("task-run-1", instance, "workflowExecution");

      expect(taskRegistry.updateStatus).toHaveBeenCalledWith("task-run-1", "RUNNING");
    });

    it("should call taskRegistry.updateStatusToCompleted on success", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("task-complete");
      await queue.submitSync("task-complete", instance, "workflowExecution");

      expect(taskRegistry.updateStatus).toHaveBeenCalledWith(
        "task-complete",
        "COMPLETED",
        expect.anything(),
      );
    });

    it("should handle task execution failure", async () => {
      executeFn.mockRejectedValue(new Error("Execution failure"));

      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("task-fail");
      await expect(queue.submitSync("task-fail", instance, "workflowExecution")).rejects.toThrow(
        "Execution failure",
      );

      expect(taskRegistry.updateStatus).toHaveBeenCalledWith(
        "task-fail",
        "FAILED",
        expect.any(Object),
      );
    });

    it("should handle multiple tasks sequentially", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const promise1 = queue.submitSync("seq-1", createWFInstance("seq-1"), "workflowExecution");
      const promise2 = queue.submitSync("seq-2", createWFInstance("seq-2"), "workflowExecution");

      const result1 = await promise1;
      const result2 = await promise2;

      expect(result1.instance.id).toBe("seq-1");
      expect(result2.instance.id).toBe("seq-2");
    });

    it("should return agentResult for agent instances", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      executeFn.mockResolvedValue({ agentOutput: "hello" });

      const instance = createAgentInstance("agent-1");
      const result = await queue.submitSync("agent-1", instance, "agent");

      expect(result.agentResult).toEqual({ agentOutput: "hello" });
      expect(result.executionResult).toBeUndefined();
    });
  });

  // -----------------------------------------------------------------------
  // submitAsync
  // -----------------------------------------------------------------------

  describe("submitAsync", () => {
    it("should return TaskSubmissionResult immediately", () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("async-task");
      const result = queue.submitAsync("async-task", instance, "workflowExecution");

      expect(result.taskId).toBe("async-task");
      expect(result.status).toBe("QUEUED");
      expect(typeof result.submitTime).toBe("number");
    });

    it("should still execute the task asynchronously", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("async-exec");
      queue.submitAsync("async-exec", instance, "workflowExecution");

      // Yield to the event loop so the async processing runs
      await new Promise(resolve => setTimeout(resolve, 50));

      expect(taskRegistry.updateStatus).toHaveBeenCalledWith("async-exec", "RUNNING");
      expect(taskRegistry.updateStatus).toHaveBeenCalledWith("async-exec", "COMPLETED", expect.anything());
    });

    it("should handle async task failure without throwing", async () => {
      executeFn.mockRejectedValue(new Error("Async fail"));

      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("async-fail");
      queue.submitAsync("async-fail", instance, "workflowExecution");

      await new Promise(resolve => setTimeout(resolve, 50));

      expect(taskRegistry.updateStatus).toHaveBeenCalledWith("async-fail", "FAILED", expect.anything());
    });
  });

  // -----------------------------------------------------------------------
  // cancelTask
  // -----------------------------------------------------------------------

  describe("cancelTask", () => {
    it("should cancel a pending task (before processQueue picks it up)", () => {
      // If processQueue is already running, a subsequent submitAsync task
      // stays in pendingQueue (isProcessing guard prevents re-entry).
      // We use a hanging executeFn to keep processQueue occupied.
      executeFn.mockImplementation(() => new Promise(() => {}));

      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      // First task occupies processQueue
      queue.submitAsync("blocker", createWFInstance("blocker"), "workflowExecution");

      // Second task stays in pendingQueue because processQueue is busy
      queue.submitAsync("cancel-me", createWFInstance("cancel-me"), "workflowExecution");

      const cancelled = queue.cancelTask("cancel-me");

      expect(cancelled).toBe(true);
      expect(taskRegistry.updateStatus).toHaveBeenCalledWith("cancel-me", "CANCELLED");
    });

    it("should return false for non-existent task", () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const cancelled = queue.cancelTask("nonexistent");
      expect(cancelled).toBe(false);
    });

    it("should return false for running task", async () => {
      // Make executeFn hang so the task stays running
      executeFn.mockImplementation(() => new Promise(() => {}));

      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("running-task");
      queue.submitAsync("running-task", instance, "workflowExecution");

      // Let the event loop pick up the processing
      await new Promise(resolve => setTimeout(resolve, 10));

      const cancelled = queue.cancelTask("running-task");
      expect(cancelled).toBe(false);
    });
  });

  // -----------------------------------------------------------------------
  // getQueueStats
  // -----------------------------------------------------------------------

  describe("getQueueStats", () => {
    it("should return initial stats", () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const stats = queue.getQueueStats();
      expect(stats.pendingCount).toBe(0);
      expect(stats.runningCount).toBe(0);
    });

    it("should reflect pending tasks when processQueue is occupied", () => {
      // processQueue shifts tasks from pending BEFORE allocating the executor.
      // To see pending > 0, processQueue must be busy processing another task.
      executeFn.mockImplementation(() => new Promise(() => {}));

      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      // First task occupies processQueue
      queue.submitAsync("stat-blocker", createWFInstance("stat-blocker"), "workflowExecution");

      // Second task stays in pendingQueue
      queue.submitAsync("stat-pending", createWFInstance("stat-pending"), "workflowExecution");

      const stats = queue.getQueueStats();
      expect(stats.pendingCount).toBe(1);
      expect(stats.runningCount).toBe(0);
    });

    it("should propagate stats from task registry", () => {
      const statsMock = { completed: 5, failed: 2, cancelled: 1, registered: 8 };
      taskRegistry.getStats = vi.fn().mockReturnValue(statsMock);

      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const stats = queue.getQueueStats();
      expect(stats.completedCount).toBe(5);
      expect(stats.failedCount).toBe(2);
      expect(stats.cancelledCount).toBe(1);
    });
  });

  // -----------------------------------------------------------------------
  // clear
  // -----------------------------------------------------------------------

  describe("clear", () => {
    it("should clear all pending tasks", () => {
      // processQueue must be occupied so tasks stay in pendingQueue
      executeFn.mockImplementation(() => new Promise(() => {}));

      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      // First task occupies processQueue
      queue.submitAsync("blocker", createWFInstance("blocker"), "workflowExecution");
      // Second task stays in pendingQueue
      queue.submitAsync("clear-1", createWFInstance("clear-1"), "workflowExecution");

      expect(queue.getQueueStats().pendingCount).toBe(1);

      queue.clear();
      expect(queue.getQueueStats().pendingCount).toBe(0);
    });

    it("should not affect already running tasks", async () => {
      executeFn.mockImplementation(() => new Promise(() => {}));

      // Use maxExecutors=1 so only one task can run at once
      const pool = createFreshPool(executorFactory, { minExecutors: 1, maxExecutors: 1 });
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      // First task becomes running
      queue.submitAsync("running-1", createWFInstance("running-1"), "workflowExecution");
      // Second task stays pending (pool is full)
      queue.submitAsync("pending-1", createWFInstance("pending-1"), "workflowExecution");

      // Let the async processQueue pick up the first task
      await new Promise(resolve => setTimeout(resolve, 10));

      queue.clear();

      const stats = queue.getQueueStats();
      expect(stats.pendingCount).toBe(0);
      expect(stats.runningCount).toBe(1);
    });
  });
  // drain
  // -----------------------------------------------------------------------

  describe("drain", () => {
    it("should resolve when no tasks are pending or running", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      await expect(queue.drain()).resolves.toBeUndefined();
    });

    it("should wait for tasks to complete", async () => {
      const pool = createFreshPool(executorFactory);
      const queue = new ExecutionQueue(
        taskRegistry,
        pool,
        eventRegistry,
        executeFn as (...args: any[]) => any,
      );

      const instance = createWFInstance("drain-1");
      const resultPromise = queue.submitSync("drain-1", instance, "workflowExecution");

      // drain should wait for the task to finish
      await queue.drain();
      const result = await resultPromise;

      const stats = queue.getQueueStats();
      expect(stats.pendingCount).toBe(0);
      expect(stats.runningCount).toBe(0);
      expect(result).toBeDefined();
    });
  });
});
