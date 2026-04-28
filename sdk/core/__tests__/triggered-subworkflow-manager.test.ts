/**
 * Triggered Subworkflow Manager Integration Testing
 *
 * Test Scenarios:
 * - Synchronous Execution
 * - Asynchronous Execution
 * - Task Management
 * - Parent-Child Thread Relationships
 * - Statistical Information
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { TriggeredSubworkflowHandler } from "../../workflow/execution/handlers/triggered-subworkflow-handler.js";
import type {
  TriggeredSubgraphTask,
  ExecutedSubgraphResult,
  TaskSubmissionResult,
} from "../../workflow/execution/types/triggered-subworkflow.types.js";
import type { WorkflowExecutionResult } from "@wf-agent/types";
import type { TaskStatus } from "@wf-agent/types";
import type { ExecutionInstanceType } from "../types/index.js";

// Mock implementations
const mockThreadRegistry = {
  register: vi.fn(),
  get: vi.fn(),
  delete: vi.fn(),
} as any;

const mockThreadBuilder = {
  build: vi.fn(),
} as any;

const mockTaskQueueManager = {
  submitSync: vi.fn(),
  submitAsync: vi.fn(),
  cancelTask: vi.fn(),
  getQueueStats: vi.fn(),
  drain: vi.fn(),
} as any;

const mockEventManager = {
  emit: vi.fn(),
} as any;

const mockThreadPoolService = {
  getConfig: vi.fn(() => ({ defaultTimeout: 60000 })),
  getStats: vi.fn(),
  shutdown: vi.fn(),
} as any;

describe("Triggered Subworkflow Manager - Triggered Subworkflow Manager", () => {
  let manager: TriggeredSubworkflowHandler;
  let mockMainThreadEntity: any;
  let mockSubgraphEntity: any;

  beforeEach(() => {
    vi.clearAllMocks();

    // Create a manager
    manager = new TriggeredSubworkflowHandler(
      mockThreadRegistry,
      mockThreadBuilder,
      mockTaskQueueManager,
      mockEventManager,
      mockThreadPoolService,
    );

    // Mock Main Thread Entity
    mockMainThreadEntity = {
      id: "main-thread-123",
      getThreadId: vi.fn(() => "main-thread-123"),
      getWorkflowId: vi.fn(() => "workflow-123"),
      getInput: vi.fn(() => ({ key: "value" })),
      getOutput: vi.fn(() => ({ result: "success" })),
      registerChildThread: vi.fn(),
      unregisterChildThread: vi.fn(),
    };

    // Mock Sub-workflow Entity
    mockSubgraphEntity = {
      id: "subgraph-thread-456",
      getThreadId: vi.fn(() => "subgraph-thread-456"),
      getWorkflowId: vi.fn(() => "subgraph-workflow-789"),
      setThreadType: vi.fn(),
      setParentThreadId: vi.fn(),
      getParentThreadId: vi.fn(() => "main-thread-123"),
      setTriggeredSubworkflowId: vi.fn(),
      getTriggeredSubworkflowId: vi.fn(() => "subgraph-1"),
    };

    // Mock threadBuilder.build
    mockThreadBuilder.build.mockResolvedValue(mockSubgraphEntity);
  });

  describe("synchronous execution", () => {
    it("Testing synchronized execution of subworkflows: waiting for completion when waitForCompletion is true", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: { customInput: "test" },
        config: {
          waitForCompletion: true,
          timeout: 60000,
        },
      };

      const mockThreadResult: WorkflowExecutionResult = {
        threadId: "subgraph-thread-456",
        output: { compressed: "context" },
        executionTime: 100,
        nodeResults: [],
        metadata: {
          status: "COMPLETED",
          startTime: Date.now(),
          endTime: Date.now() + 100,
          executionTime: 100,
          nodeCount: 0,
          errorCount: 0,
        },
      };

      const expectedResult: ExecutedSubgraphResult = {
        subgraphEntity: mockSubgraphEntity,
        threadResult: mockThreadResult,
        executionTime: 100,
      };

      mockTaskQueueManager.submitSync.mockResolvedValue(expectedResult);

      const result = await manager.executeTriggeredSubgraph(task);

      expect(mockThreadBuilder.build).toHaveBeenCalledWith("subgraph-1", {
        input: expect.objectContaining({
          triggerId: "trigger-1",
          output: mockMainThreadEntity.getOutput(),
          input: mockMainThreadEntity.getInput(),
          customInput: "test",
        }),
      });

      expect(mockThreadRegistry.register).toHaveBeenCalledWith(mockSubgraphEntity);
      expect(mockMainThreadEntity.registerChildThread).toHaveBeenCalledWith("subgraph-thread-456");
      expect(mockSubgraphEntity.setParentThreadId).toHaveBeenCalledWith("main-thread-123");
      expect(mockSubgraphEntity.setTriggeredSubworkflowId).toHaveBeenCalledWith("subgraph-1");

      expect(result).toEqual(expectedResult);
    });

    it("Test execution results returned: correctly returned execution results", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        config: {
          waitForCompletion: true,
        },
      };

      const mockThreadResult: WorkflowExecutionResult = {
        threadId: "subgraph-thread-456",
        output: { data: "test" },
        executionTime: 200,
        nodeResults: [],
        metadata: {
          status: "COMPLETED",
          startTime: Date.now(),
          endTime: Date.now() + 200,
          executionTime: 200,
          nodeCount: 0,
          errorCount: 0,
        },
      };

      const expectedResult: ExecutedSubgraphResult = {
        subgraphEntity: mockSubgraphEntity,
        threadResult: mockThreadResult,
        executionTime: 200,
      };

      mockTaskQueueManager.submitSync.mockResolvedValue(expectedResult);

      const result = await manager.executeTriggeredSubgraph(task);

      expect(result).toBe(expectedResult);
    });

    it("Test timeout handling: correct handling in case of timeout", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        config: {
          waitForCompletion: true,
          timeout: 1000,
        },
      };

      mockTaskQueueManager.submitSync.mockRejectedValue(new Error("Timeout"));

      await expect(manager.executeTriggeredSubgraph(task)).rejects.toThrow("Timeout");
    });
  });

  describe("asynchronous execution", () => {
    it("Testing asynchronous submission of subworkflows: asynchronous execution when waitForCompletion is false", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        config: {
          waitForCompletion: false,
        },
      };

      const submissionResult = {
        taskId: "task-123",
        status: "PENDING",
        submitTime: Date.now(),
      };

      mockTaskQueueManager.submitAsync.mockReturnValue(submissionResult);

      const result = await manager.executeTriggeredSubgraph(task);

      expect(result).toEqual({
        taskId: "task-123",
        status: "PENDING",
        message: "Triggered subgraph submitted",
        submitTime: submissionResult.submitTime,
      });

      expect(mockTaskQueueManager.submitAsync).toHaveBeenCalled();
    });

    it("Test task submission results: return task ID and status", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        config: {
          waitForCompletion: false,
        },
      };

      const submissionResult = {
        taskId: "task-456",
        status: "QUEUED",
        submitTime: Date.now(),
      };

      mockTaskQueueManager.submitAsync.mockReturnValue(submissionResult);

      const result = await manager.executeTriggeredSubgraph(task);

      // Add type checking
      expect(result).toEqual({
        taskId: "task-456",
        status: "QUEUED",
        message: "Triggered subgraph submitted",
        submitTime: expect.any(Number),
      });
    });

    it("Test default synchronized execution: default synchronization when waitForCompletion is not specified", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        // The `config` setting is not configured.
      };

      const mockThreadResult: WorkflowExecutionResult = {
        threadId: "subgraph-thread-456",
        output: {},
        executionTime: 50,
        nodeResults: [],
        metadata: {
          status: "COMPLETED",
          startTime: Date.now(),
          endTime: Date.now() + 50,
          executionTime: 50,
          nodeCount: 0,
          errorCount: 0,
        },
      };

      const expectedResult: ExecutedSubgraphResult = {
        subgraphEntity: mockSubgraphEntity,
        threadResult: mockThreadResult,
        executionTime: 50,
      };

      mockTaskQueueManager.submitSync.mockResolvedValue(expectedResult);

      await manager.executeTriggeredSubgraph(task);

      expect(mockTaskQueueManager.submitSync).toHaveBeenCalled();
    });
  });

  describe("task management", () => {
    it("Test query task status: getTaskStatus returns the correct task status", () => {
      const mockTaskInfo = {
        id: "task-123",
        status: "RUNNING" as TaskStatus,
        instanceType: "thread" as ExecutionInstanceType,
        instance: mockSubgraphEntity,
        threadEntity: mockSubgraphEntity,
        taskManager: manager,
        submitTime: Date.now(),
      };

      manager["taskRegistry"]["register"] = vi.fn(() => "task-123");
      manager["taskRegistry"]["get"] = vi.fn(() => mockTaskInfo);

      const taskStatus = manager.getTaskStatus("task-123");

      expect(taskStatus).toBe(mockTaskInfo);
    });

    it("Test for canceling a task: cancelTask correctly cancels the task", async () => {
      mockTaskQueueManager.cancelTask.mockReturnValue(true);

      const success = await manager.cancelTask("task-123");

      expect(success).toBe(true);
      expect(mockTaskQueueManager.cancelTask).toHaveBeenCalledWith("task-123");
    });

    it("Test cleaning up expired tasks: cleanupExpiredTasks cleanupExpiredTasks", () => {
      const retentionTime = 60000; // 1 minute

      manager["taskRegistry"]["cleanup"] = vi.fn(() => Promise.resolve(5));

      const cleanedCount = manager.cleanupExpiredTasks(retentionTime);

      expect(manager["taskRegistry"]["cleanup"]).toHaveBeenCalledWith(retentionTime);
      expect(cleanedCount).toBe(5);
    });
  });

  describe("Parent-child thread relationship", () => {
    it("Testing Establishing Parent-Child Relationships: Correctly Establishing Parent-Child Thread Relationships", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        config: {
          waitForCompletion: true,
        },
      };

      const mockThreadResult: WorkflowExecutionResult = {
        threadId: "subgraph-thread-456",
        output: {},
        executionTime: 50,
        nodeResults: [],
        metadata: {
          status: "COMPLETED",
          startTime: Date.now(),
          endTime: Date.now() + 50,
          executionTime: 50,
          nodeCount: 0,
          errorCount: 0,
        },
      };

      mockTaskQueueManager.submitSync.mockResolvedValue({
        subgraphEntity: mockSubgraphEntity,
        threadResult: mockThreadResult,
        executionTime: 50,
      });

      await manager.executeTriggeredSubgraph(task);

      // Verify that the parent-child relationship has been established.
      expect(mockMainThreadEntity.registerChildThread).toHaveBeenCalledWith("subgraph-thread-456");
      expect(mockSubgraphEntity.setParentThreadId).toHaveBeenCalledWith("main-thread-123");
      expect(mockSubgraphEntity.setTriggeredSubworkflowId).toHaveBeenCalledWith("subgraph-1");
    });

    it("Test deregistering a parent-child relationship: correctly deregister the relationship upon completion", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        config: {
          waitForCompletion: true,
        },
      };

      const mockThreadResult: WorkflowExecutionResult = {
        threadId: "subgraph-thread-456",
        output: {},
        executionTime: 50,
        nodeResults: [],
        metadata: {
          status: "COMPLETED",
          startTime: Date.now(),
          endTime: Date.now() + 50,
          executionTime: 50,
          nodeCount: 0,
          errorCount: 0,
        },
      };

      mockTaskQueueManager.submitSync.mockResolvedValue({
        subgraphEntity: mockSubgraphEntity,
        threadResult: mockThreadResult,
        executionTime: 50,
      });

      // 设置 mockThreadRegistry.get 返回 mockMainThreadEntity
      mockThreadRegistry.get.mockReturnValue(mockMainThreadEntity);

      await manager.executeTriggeredSubgraph(task);

      // Verify that the parent-child relationship has been canceled.
      expect(mockMainThreadEntity.unregisterChildThread).toHaveBeenCalledWith(
        "subgraph-thread-456",
      );
    });
  });

  describe("Statistical information", () => {
    it("Test queue statistics: getQueueStats returns the correct queue statistics", () => {
      const queueStats = {
        pending: 5,
        running: 2,
        completed: 10,
        failed: 1,
      };

      mockTaskQueueManager.getQueueStats.mockReturnValue(queueStats);

      const stats = manager.getQueueStats();

      expect(stats).toBe(queueStats);
      expect(mockTaskQueueManager.getQueueStats).toHaveBeenCalled();
    });

    it("Testing thread pool statistics: getPoolStats returns the correct thread pool statistics", () => {
      const poolStats = {
        activeThreads: 3,
        idleThreads: 2,
        totalThreads: 5,
        maxThreads: 10,
      };

      mockThreadPoolService.getStats.mockReturnValue(poolStats);

      const stats = manager.getPoolStats();

      expect(stats).toBe(poolStats);
      expect(mockThreadPoolService.getStats).toHaveBeenCalled();
    });

    it("Test task statistics: getTaskStats returns the correct task statistics", () => {
      const taskStats = {
        total: 20,
        queued: 5,
        running: 3,
        completed: 10,
        failed: 2,
        cancelled: 0,
        timeout: 0,
      };

      manager["taskRegistry"]["getStats"] = vi.fn(() => taskStats);

      const stats = manager.getTaskStats();

      expect(stats).toBe(taskStats);
      expect(manager["taskRegistry"]["getStats"]).toHaveBeenCalled();
    });
  });

  describe("Close Manager", () => {
    it("Test shutdown manager: shutdown method shuts down correctly", async () => {
      mockTaskQueueManager.drain.mockResolvedValue(undefined);
      mockThreadPoolService.shutdown.mockResolvedValue(undefined);

      await manager.shutdown();

      expect(mockThreadPoolService.shutdown).toHaveBeenCalled();
      expect(mockTaskQueueManager.drain).toHaveBeenCalled();
    });
  });

  describe("Boundary situation", () => {
    it("Test for missing subgraphId: should throw error", async () => {
      const task: any = {
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        // `subgraphId` is missing.
      };

      await expect(manager.executeTriggeredSubgraph(task)).rejects.toThrow();
    });

    it("Test missing mainThreadEntity: should throw error", async () => {
      const task: any = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        // `mainThreadEntity` is missing.
      };

      await expect(manager.executeTriggeredSubgraph(task)).rejects.toThrow();
    });

    it("Error handling when test execution fails: errors should be handled correctly", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainThreadEntity: mockMainThreadEntity,
        input: {},
        config: {
          waitForCompletion: true,
        },
      };

      mockTaskQueueManager.submitSync.mockRejectedValue(new Error("Execution failed"));

      await expect(manager.executeTriggeredSubgraph(task)).rejects.toThrow("Execution failed");
    });
  });
});
