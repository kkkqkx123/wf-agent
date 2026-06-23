/**
 * Tests for TaskQueue
 */

import { describe, it, expect, vi, beforeEach, Mock } from "vitest";
import { TaskQueue } from "../task-queue.js";
import { TaskRegistry } from "../task-registry.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { TaskManager } from "../task-registry.js";

// Mock dependencies
vi.mock("../../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("@wf-agent/common-utils", async importOriginal => {
  const actual = await importOriginal();
  return {
    ...(actual as Record<string, unknown>),
    getErrorMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
    getErrorOrNew: (error: unknown) => (error instanceof Error ? error : new Error(String(error))),
    now: () => 1000,
    diffTimestamp: () => 500,
  };
});

vi.mock("../../../../utils/index.js", () => ({
  generateId: () => "generated-task-id",
}));

vi.mock("../../../../shared/utils/event/emit-event.js", () => ({
  emit: vi.fn(),
}));

vi.mock("../../../../shared/utils/event/builders/index.js", () => ({
  buildTriggeredSubgraphCompletedEvent: vi.fn(() => ({ type: "subgraph.completed" })),
  buildTriggeredSubgraphFailedEvent: vi.fn(() => ({ type: "subgraph.failed" })),
}));

vi.mock("../../../../shared/utils/error-utils.js", () => ({
  logError: vi.fn(),
  emitErrorEvent: vi.fn(),
}));

function createMockExecutionEntity(id: string): WorkflowExecutionEntity {
  return {
    id,
    getWorkflowId: vi.fn(() => "wf-1"),
    getTriggeredSubworkflowId: vi.fn(() => "subgraph-1"),
    getOutput: vi.fn(() => ({ result: "ok" })),
  } as unknown as WorkflowExecutionEntity;
}

function createMockExecutor() {
  return {
    executeWorkflow: vi.fn(),
  };
}

function createMockWorkflowExecutionPool() {
  return {
    allocateExecutor: vi.fn(),
    releaseExecutor: vi.fn(),
  };
}

function createMockEventRegistry(): EventRegistry {
  return {} as unknown as EventRegistry;
}

describe("TaskQueue", () => {
  let taskRegistry: TaskRegistry;
  let taskManager: TaskManager;
  let workflowExecutionPool: ReturnType<typeof createMockWorkflowExecutionPool>;
  let eventRegistry: EventRegistry;
  let taskQueue: TaskQueue;

  beforeEach(() => {
    taskRegistry = new TaskRegistry();
    taskManager = {
      cancelTask: vi.fn().mockResolvedValue(true),
      getTaskStatus: vi.fn(),
    };
    workflowExecutionPool = createMockWorkflowExecutionPool();
    eventRegistry = createMockEventRegistry();
    taskQueue = new TaskQueue(taskRegistry, workflowExecutionPool as any, eventRegistry);

    vi.clearAllMocks();
  });

  describe("constructor", () => {
    it("should create a task queue", () => {
      expect(taskQueue).toBeDefined();
      const stats = taskQueue.getQueueStats();
      expect(stats.pendingCount).toBe(0);
      expect(stats.runningCount).toBe(0);
    });
  });

  describe("submitSync", () => {
    it("should submit a synchronous task", async () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      const executor = createMockExecutor();
      (executor.executeWorkflow as Mock).mockResolvedValue({
        executionId: "exec-1",
        output: {},
        executionTime: 500,
        nodeResults: [],
        metadata: {
          status: "COMPLETED" as const,
          startTime: 0,
          endTime: 500,
          executionTime: 500,
          nodeCount: 0,
          errorCount: 0,
        },
      });
      (workflowExecutionPool.allocateExecutor as Mock).mockResolvedValue(executor);
      (workflowExecutionPool.releaseExecutor as Mock).mockResolvedValue(undefined);

      const resultPromise = taskQueue.submitSync(taskId, entity, 5000);

      // Wait for promise resolution
      const result = await resultPromise;

      expect(result).toBeDefined();
      expect(result.subworkflowEntity).toBe(entity);
      expect(result.executionResult.metadata.status).toBe("COMPLETED");
      expect(result.executionTime).toBe(500);
    });

    it("should handle execution failure", async () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      const executor = createMockExecutor();
      (executor.executeWorkflow as Mock).mockRejectedValue(new Error("Execution failed"));
      (workflowExecutionPool.allocateExecutor as Mock).mockResolvedValue(executor);
      (workflowExecutionPool.releaseExecutor as Mock).mockResolvedValue(undefined);

      await expect(taskQueue.submitSync(taskId, entity, 5000)).rejects.toThrow("Execution failed");
    });

    it("should queue when pool is busy", async () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      const executor = createMockExecutor();
      (executor.executeWorkflow as Mock).mockResolvedValue({
        executionId: "exec-1",
        output: {},
        executionTime: 500,
        nodeResults: [],
        metadata: {
          status: "COMPLETED" as const,
          startTime: 0,
          endTime: 500,
          executionTime: 500,
          nodeCount: 0,
          errorCount: 0,
        },
      });
      (workflowExecutionPool.allocateExecutor as Mock).mockResolvedValue(executor);
      (workflowExecutionPool.releaseExecutor as Mock).mockResolvedValue(undefined);

      const resultPromise = taskQueue.submitSync(taskId, entity, 5000);

      // Task stays in pendingQueue until after the async await resolves
      const stats = taskQueue.getQueueStats();
      expect(stats.pendingCount).toBe(1);

      const result = await resultPromise;
      expect(result).toBeDefined();
    });
  });

  describe("submitAsync", () => {
    it("should submit an asynchronous task", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      const result = taskQueue.submitAsync(taskId, entity, 5000);

      expect(result.taskId).toBe(taskId);
      expect(result.status).toBe("QUEUED");
    });

    it("should return submission result", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      const result = taskQueue.submitAsync(taskId, entity);

      expect(result).toHaveProperty("taskId", taskId);
      expect(result).toHaveProperty("status", "QUEUED");
      expect(result).toHaveProperty("submitTime");
    });
  });

  describe("cancelTask", () => {
    it("should cancel a pending task", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      // Submit async task to put it in the queue
      taskQueue.submitAsync(taskId, entity, 5000);

      const result = taskQueue.cancelTask(taskId);
      expect(result).toBe(true);
    });

    it("should return false for non-existent task", () => {
      const result = taskQueue.cancelTask("non-existent");
      expect(result).toBe(false);
    });

    it("should return false for running task", async () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      const executor = createMockExecutor();
      // Don't resolve the executeWorkflow promise so task stays running
      (executor.executeWorkflow as Mock).mockReturnValue(new Promise(() => {}));
      (workflowExecutionPool.allocateExecutor as Mock).mockResolvedValue(executor);

      // Submit sync which will start processing
      taskQueue.submitAsync(taskId, entity);

      // Wait a tick for processing to begin
      await new Promise(resolve => setTimeout(resolve, 50));

      const result = taskQueue.cancelTask(taskId);
      expect(result).toBe(false); // Running task cannot be cancelled
    });
  });

  describe("getQueueStats", () => {
    it("should return correct stats", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = taskRegistry.register(entity, "workflowExecution", taskManager);

      taskQueue.submitAsync(taskId, entity);

      const stats = taskQueue.getQueueStats();
      expect(stats).toHaveProperty("pendingCount");
      expect(stats).toHaveProperty("runningCount");
    });
  });

  describe("clear", () => {
    it("should clear all pending tasks", () => {
      const entity1 = createMockExecutionEntity("exec-1");
      const entity2 = createMockExecutionEntity("exec-2");
      const id1 = taskRegistry.register(entity1, "workflowExecution", taskManager);
      const id2 = taskRegistry.register(entity2, "workflowExecution", taskManager);

      taskQueue.submitAsync(id1, entity1);
      taskQueue.submitAsync(id2, entity2);

      taskQueue.clear();

      const stats = taskQueue.getQueueStats();
      expect(stats.pendingCount).toBe(0);
    });
  });

  describe("drain", () => {
    it("should resolve when no tasks are pending or running", async () => {
      await expect(taskQueue.drain()).resolves.toBeUndefined();
    });
  });
});
