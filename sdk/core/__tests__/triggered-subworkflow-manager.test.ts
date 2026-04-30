/**
 * Triggered Subworkflow Manager Integration Testing
 *
 * Test Scenarios:
 * - Synchronous Execution
 * - Asynchronous Execution
 * - Task Management
 * - Parent-Child Workflow Execution Relationships
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
const mockWorkflowExecutionRegistry = {
  register: vi.fn(),
  get: vi.fn(),
  delete: vi.fn(),
} as any;

const mockWorkflowExecutionBuilder = {
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

const mockWorkflowExecutionPoolService = {
  getConfig: vi.fn(() => ({ defaultTimeout: 60000 })),
  getStats: vi.fn(),
  shutdown: vi.fn(),
} as any;

describe("Triggered Subworkflow Manager - Triggered Subworkflow Manager", () => {
  let manager: TriggeredSubworkflowHandler;
  let mockMainWorkflowExecutionEntity: any;
  let mockSubgraphEntity: any;

  beforeEach(() => {
    vi.clearAllMocks();

    // Create a manager
    manager = new TriggeredSubworkflowHandler(
      mockWorkflowExecutionRegistry,
      mockWorkflowExecutionBuilder,
      mockTaskQueueManager,
      mockEventManager,
      mockWorkflowExecutionPoolService,
    );

    // Mock Main Workflow Execution Entity
    mockMainWorkflowExecutionEntity = {
      id: "main-execution-123",
      getExecutionId: vi.fn(() => "main-workflow-execution-123"),
      getWorkflowId: vi.fn(() => "workflow-123"),
      getInput: vi.fn(() => ({ key: "value" })),
      getOutput: vi.fn(() => ({ result: "success" })),
      registerChildExecution: vi.fn(),
      unregisterChildExecution: vi.fn(),
    };

    // Mock Sub-workflow Entity
    mockSubgraphEntity = {
      id: "subgraph-workflow-execution-456",
      getExecutionId: vi.fn(() => "subgraph-workflow-execution-456"),
      getWorkflowId: vi.fn(() => "subgraph-workflow-789"),
      setWorkflowExecutionType: vi.fn(),
      setParentExecutionId: vi.fn(),
      getParentWorkflowExecutionId: vi.fn(() => "main-workflow-execution-123"),
      setTriggeredSubworkflowId: vi.fn(),
      getTriggeredSubworkflowId: vi.fn(() => "subgraph-1"),
    };

    // Mock executionBuilder.build
    mockWorkflowExecutionBuilder.build.mockResolvedValue(mockSubgraphEntity);
  });

  describe("synchronous execution", () => {
    it("Testing synchronized execution of subworkflows: waiting for completion when waitForCompletion is true", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
        input: { customInput: "test" },
        config: {
          waitForCompletion: true,
          timeout: 60000,
        },
      };

      const mockWorkflowExecutionResult: WorkflowExecutionResult = {
        executionId: "subgraph-workflow-execution-456",
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
        executionResult: mockWorkflowExecutionResult,
        executionTime: 100,
      };

      mockTaskQueueManager.submitSync.mockResolvedValue(expectedResult);

      const result = await manager.executeTriggeredSubgraph(task);

      expect(mockWorkflowExecutionBuilder.build).toHaveBeenCalledWith("subgraph-1", {
        input: expect.objectContaining({
          triggerId: "trigger-1",
          output: mockMainWorkflowExecutionEntity.getOutput(),
          input: mockMainWorkflowExecutionEntity.getInput(),
          customInput: "test",
        }),
      });

      expect(mockWorkflowExecutionRegistry.register).toHaveBeenCalledWith(mockSubgraphEntity);
      expect(mockMainWorkflowExecutionEntity.registerChildExecution).toHaveBeenCalledWith("subgraph-workflow-execution-456");
      expect(mockSubgraphEntity.setParentWorkflowExecutionId).toHaveBeenCalledWith("main-workflow-execution-123");
      expect(mockSubgraphEntity.setTriggeredSubworkflowId).toHaveBeenCalledWith("subgraph-1");

      expect(result).toEqual(expectedResult);
    });

    it("Test execution results returned: correctly returned execution results", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
        input: {},
        config: {
          waitForCompletion: true,
        },
      };

      const mockWorkflowExecutionResult: WorkflowExecutionResult = {
        executionId: "subgraph-workflow-execution-456",
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
        executionResult: mockWorkflowExecutionResult,
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
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
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
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
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
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
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
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
        input: {},
        // The `config` setting is not configured.
      };

      const mockWorkflowExecutionResult: WorkflowExecutionResult = {
        executionId: "subgraph-workflow-execution-456",
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
        executionResult: mockWorkflowExecutionResult,
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
        instanceType: "workflowExecution" as ExecutionInstanceType,
        instance: mockSubgraphEntity,
        executionEntity: mockSubgraphEntity,
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

  describe("Parent-child workflow execution relationship", () => {
    it("Testing Establishing Parent-Child Relationships: Correctly Establishing Parent-Child Workflow Execution Relationships", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
        input: {},
        config: {
          waitForCompletion: true,
        },
      };

      const mockWorkflowExecutionResult: WorkflowExecutionResult = {
        executionId: "subgraph-workflow-execution-456",
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
        executionResult: mockWorkflowExecutionResult,
        executionTime: 50,
      });

      await manager.executeTriggeredSubgraph(task);

      // Verify that the parent-child relationship has been established.
      expect(mockMainWorkflowExecutionEntity.registerChildExecution).toHaveBeenCalledWith("subgraph-workflow-execution-456");
      expect(mockSubgraphEntity.setParentWorkflowExecutionId).toHaveBeenCalledWith("main-workflow-execution-123");
      expect(mockSubgraphEntity.setTriggeredSubworkflowId).toHaveBeenCalledWith("subgraph-1");
    });

    it("Test deregistering a parent-child relationship: correctly deregister the relationship upon completion", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
        input: {},
        config: {
          waitForCompletion: true,
        },
      };

      const mockWorkflowExecutionResult: WorkflowExecutionResult = {
        executionId: "subgraph-workflow-execution-456",
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
        executionResult: mockWorkflowExecutionResult,
        executionTime: 50,
      });

      // 设置 mockWorkflowExecutionRegistry.get 返回 mockMainWorkflowExecutionEntity
      mockWorkflowExecutionRegistry.get.mockReturnValue(mockMainWorkflowExecutionEntity);

      await manager.executeTriggeredSubgraph(task);

      // Verify that the parent-child relationship has been canceled.
      expect(mockMainWorkflowExecutionEntity.unregisterChildExecution).toHaveBeenCalledWith(
        "subgraph-workflow-execution-456",
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

    it("Testing workflow execution pool statistics: getPoolStats returns the correct workflow execution pool statistics", () => {
      const poolStats = {
        activeExecutions: 3,
        idleExecutions: 2,
        totalExecutions: 5,
        maxExecutions: 10,
      };

      mockWorkflowExecutionPoolService.getStats.mockReturnValue(poolStats);

      const stats = manager.getPoolStats();

      expect(stats).toBe(poolStats);
      expect(mockWorkflowExecutionPoolService.getStats).toHaveBeenCalled();
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
      mockWorkflowExecutionPoolService.shutdown.mockResolvedValue(undefined);

      await manager.shutdown();

      expect(mockWorkflowExecutionPoolService.shutdown).toHaveBeenCalled();
      expect(mockTaskQueueManager.drain).toHaveBeenCalled();
    });
  });

  describe("Boundary situation", () => {
    it("Test for missing subgraphId: should throw error", async () => {
      const task: any = {
        triggerId: "trigger-1",
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
        // `subgraphId` is missing.
      };

      await expect(manager.executeTriggeredSubgraph(task)).rejects.toThrow();
    });

    it("Test missing mainWorkflowExecutionEntity: should throw error", async () => {
      const task: any = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        // `mainWorkflowExecutionEntity` is missing.
      };

      await expect(manager.executeTriggeredSubgraph(task)).rejects.toThrow();
    });

    it("Error handling when test execution fails: errors should be handled correctly", async () => {
      const task: TriggeredSubgraphTask = {
        subgraphId: "subgraph-1",
        triggerId: "trigger-1",
        mainWorkflowExecutionEntity: mockMainWorkflowExecutionEntity,
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
