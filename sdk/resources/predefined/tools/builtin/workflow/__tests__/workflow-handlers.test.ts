/**
 * Workflow Builtin Tools Handler Tests
 *
 * Test Scenarios:
 * - Execute Workflow Handler
 * - Query Workflow Status Handler
 * - Cancel Workflow Handler
 * - Error Handling
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createExecuteWorkflowHandler } from "../execute-workflow/handler.js";
import { createQueryWorkflowStatusHandler } from "../query-workflow-status/handler.js";
import { createCancelWorkflowHandler } from "../cancel-workflow/handler.js";
import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { initializeContainerWithAdapters, resetContainer, getContainer } from "../../../../../../core/di/container-config.js";

// Mock storage callback
const mockStorageCallback = {
  save: vi.fn(),
  load: vi.fn(),
  delete: vi.fn(),
  list: vi.fn(),
  exists: vi.fn(),
  getMetadata: vi.fn(),
  initialize: vi.fn(),
  close: vi.fn(),
  clear: vi.fn(),
  getMetrics: vi.fn().mockResolvedValue({
    operationCounts: { save: 0, load: 0, delete: 0, list: 0, exists: 0, getMetadata: 0 },
    timings: { save: 0, load: 0, delete: 0, list: 0, exists: 0, getMetadata: 0 },
    sizes: { totalDataSize: 0, averageDataSize: 0 },
  }),
  resetMetrics: vi.fn(),
  saveBatch: vi.fn(),
  loadBatch: vi.fn().mockResolvedValue([]),
  deleteBatch: vi.fn(),
};

// Mock TriggeredSubworkflowHandler
const mockTriggeredSubworkflowManager = {
  executeTriggeredSubgraph: vi.fn(),
  getTaskStatus: vi.fn(),
  cancelTask: vi.fn(),
};

describe("Workflow Builtin Tools Handlers", () => {
  let mockContext: BuiltinToolExecutionContext;

  beforeEach(() => {
    // Reset and initialize container before each test
    resetContainer();
    initializeContainerWithAdapters({ checkpoint: mockStorageCallback });

    // Patch container.get to return mock for TriggeredSubworkflowHandler
    const container = getContainer();
    const originalGet = container.get.bind(container);
    (container as any).get = (serviceId: any) => {
      if (serviceId.toString().includes("TriggeredSubworkflowHandler")) {
        return mockTriggeredSubworkflowManager;
      }
      return originalGet(serviceId);
    };

    vi.clearAllMocks();

    // Mock context
    mockContext = {
      executionId: "test-workflow-execution-123",
      parentExecutionEntity: {
        id: "parent-workflow-execution-123",
        getExecutionId: vi.fn(() => "parent-workflow-execution-123"),
        getWorkflowId: vi.fn(() => "workflow-123"),
        getInput: vi.fn(() => ({ key: "value" })),
        getOutput: vi.fn(() => ({ result: "success" })),
        registerChildExecution: vi.fn(),
        unregisterChildExecution: vi.fn(),
      } as any,
    };
  });

  afterEach(() => {
    resetContainer();
  });

  describe("Execute Workflow Handler", () => {
    it("should execute workflow synchronously with waitForCompletion=true", async () => {
      const handler = createExecuteWorkflowHandler();

      const mockResult = {
        subgraphEntity: {
          getOutput: vi.fn(() => ({ data: "test" })),
        },
        workflowExecutionResult: {
          executionId: "subgraph-wfexec-456",
          output: { data: "test" },
          executionTime: 100,
        },
        executionTime: 100,
      };

      mockTriggeredSubworkflowManager.executeTriggeredSubgraph.mockResolvedValue(mockResult);

      const result = await handler(
        {
          workflowId: "test-workflow",
          input: { customInput: "test" },
          waitForCompletion: true,
        },
        mockContext,
      );

      expect(result.success).toBe(true);
      expect(result.status).toBe("completed");
      if (result.status === "completed") {
        expect(result.output).toEqual({ data: "test" });
        expect(result.executionTime).toBe(100);
      }
      expect(mockTriggeredSubworkflowManager.executeTriggeredSubgraph).toHaveBeenCalledWith(
        expect.objectContaining({
          subgraphId: "test-workflow",
          input: { customInput: "test" },
          mainWorkflowExecutionEntity: mockContext.parentExecutionEntity,
          config: expect.objectContaining({
            waitForCompletion: true,
          }),
        }),
      );
    });

    it("should execute workflow asynchronously with waitForCompletion=false", async () => {
      const handler = createExecuteWorkflowHandler();

      const mockSubmissionResult = {
        taskId: "task-123",
        status: "PENDING",
        message: "Task submitted",
        submitTime: Date.now(),
      };

      mockTriggeredSubworkflowManager.executeTriggeredSubgraph.mockResolvedValue(
        mockSubmissionResult,
      );

      const result = await handler(
        {
          workflowId: "test-workflow",
          input: {},
          waitForCompletion: false,
        },
        mockContext,
      );

      expect(result.success).toBe(true);
      expect(result.status).toBe("submitted");
      if (result.status === "submitted") {
        expect(result.taskId).toBe("task-123");
        expect(result.message).toBe("Task submitted");
      }
    });

    it("should throw error when workflowId is missing", async () => {
      const handler = createExecuteWorkflowHandler();

      await expect(
        handler(
          {
            input: {},
          },
          mockContext,
        ),
      ).rejects.toThrow(RuntimeValidationError);
    });

    it("should throw error when parentExecutionEntity is missing", async () => {
      const handler = createExecuteWorkflowHandler();

      const invalidContext = {
        executionId: "test-workflow-execution-123",
        // parentExecutionEntity is missing
      };

      await expect(
        handler(
          {
            workflowId: "test-workflow",
          },
          invalidContext as any,
        ),
      ).rejects.toThrow(RuntimeValidationError);
    });
  });

  describe("Query Workflow Status Handler", () => {
    it("should query task status successfully", async () => {
      const handler = createQueryWorkflowStatusHandler();

      // Create a mock workflow execution entity
      const mockExecutionEntity = {
        id: "wfexec-456",
        getExecutionId: vi.fn(() => "wfexec-456"),
        getWorkflowId: vi.fn(() => "workflow-789"),
      } as any;

      const mockTaskInfo = {
        id: "task-123",
        instanceType: "workflowExecution" as const,
        instance: mockExecutionEntity,
        status: "RUNNING" as const,
        submitTime: Date.now(),
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      mockTriggeredSubworkflowManager.getTaskStatus.mockReturnValue(mockTaskInfo);

      const result = await handler({ taskId: "task-123" }, mockContext);

      expect(result.success).toBe(true);
      expect(result.status).toBe("RUNNING");
      expect(result.executionId).toBe("wfexec-456");
      expect(result.workflowId).toBe("workflow-789");
      expect(mockTriggeredSubworkflowManager.getTaskStatus).toHaveBeenCalledWith("task-123");
    });

    it("should return not_found when task does not exist", async () => {
      const handler = createQueryWorkflowStatusHandler();

      mockTriggeredSubworkflowManager.getTaskStatus.mockReturnValue(null);

      const result = await handler({ taskId: "non-existent-task" }, mockContext);

      expect(result.success).toBe(false);
      expect(result.status).toBe("not_found");
      expect(result.message).toContain("not found");
    });

    it("should throw error when taskId is missing", async () => {
      const handler = createQueryWorkflowStatusHandler();

      await expect(handler({}, mockContext)).rejects.toThrow(RuntimeValidationError);
    });
  });

  describe("Cancel Workflow Handler", () => {
    it("should cancel task successfully", async () => {
      const handler = createCancelWorkflowHandler();

      mockTriggeredSubworkflowManager.cancelTask.mockResolvedValue(true);

      const result = await handler({ taskId: "task-123" }, mockContext);

      expect(result.success).toBe(true);
      expect(result.taskId).toBe("task-123");
      expect(result.message).toBe("Task cancelled successfully");
      expect(mockTriggeredSubworkflowManager.cancelTask).toHaveBeenCalledWith("task-123");
    });

    it("should handle failed cancellation", async () => {
      const handler = createCancelWorkflowHandler();

      mockTriggeredSubworkflowManager.cancelTask.mockResolvedValue(false);

      const result = await handler({ taskId: "task-123" }, mockContext);

      expect(result.success).toBe(false);
      expect(result.taskId).toBe("task-123");
      expect(result.message).toBe("Failed to cancel task");
    });

    it("should throw error when taskId is missing", async () => {
      const handler = createCancelWorkflowHandler();

      await expect(handler({}, mockContext)).rejects.toThrow(RuntimeValidationError);
    });
  });

  describe("Error Handling", () => {
    it("should provide detailed error context for execute_workflow", async () => {
      const handler = createExecuteWorkflowHandler();

      try {
        await handler({}, mockContext);
        expect.fail("Should throw error");
      } catch (error) {
        expect(error).toBeInstanceOf(RuntimeValidationError);
        const validationError = error as RuntimeValidationError;
        expect(validationError.message).toContain("workflowId is required");
        expect(validationError.context).toHaveProperty("operation", "execute_workflow");
        expect(validationError.context).toHaveProperty("field", "workflowId");
      }
    });

    it("should provide detailed error context for query_workflow_status", async () => {
      const handler = createQueryWorkflowStatusHandler();

      try {
        await handler({}, mockContext);
        expect.fail("Should throw error");
      } catch (error) {
        expect(error).toBeInstanceOf(RuntimeValidationError);
        const validationError = error as RuntimeValidationError;
        expect(validationError.message).toContain("taskId is required");
        expect(validationError.context).toHaveProperty("operation", "query_workflow_status");
        expect(validationError.context).toHaveProperty("field", "taskId");
      }
    });

    it("should provide detailed error context for cancel_workflow", async () => {
      const handler = createCancelWorkflowHandler();

      try {
        await handler({}, mockContext);
        expect.fail("Should throw error");
      } catch (error) {
        expect(error).toBeInstanceOf(RuntimeValidationError);
        const validationError = error as RuntimeValidationError;
        expect(validationError.message).toContain("taskId is required");
        expect(validationError.context).toHaveProperty("operation", "cancel_workflow");
        expect(validationError.context).toHaveProperty("field", "taskId");
      }
    });
  });
});
