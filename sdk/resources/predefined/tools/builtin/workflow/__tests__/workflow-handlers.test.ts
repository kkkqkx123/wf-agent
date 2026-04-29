/**
 * Workflow Builtin Tools Handler Tests
 *
 * Test Scenarios:
 * - Execute Workflow Handler
 * - Query Workflow Status Handler
 * - Cancel Workflow Handler
 * - Error Handling
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { createExecuteWorkflowHandler } from "../execute-workflow/handler.js";
import { createQueryWorkflowStatusHandler } from "../query-workflow-status/handler.js";
import { createCancelWorkflowHandler } from "../cancel-workflow/handler.js";
import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";

// Mock DI container
const mockTriggeredSubworkflowManager = {
  executeTriggeredSubgraph: vi.fn(),
  getTaskStatus: vi.fn(),
  cancelTask: vi.fn(),
};

const mockContainer = {
  get: vi.fn((identifier: any) => {
    if (identifier.toString().includes("TriggeredSubworkflowHandler")) {
      return mockTriggeredSubworkflowManager;
    }
    return null;
  }),
};

// Mock getContainer
vi.mock("../../../../../core/di/index.js", () => ({
  getContainer: () => mockContainer,
}));

describe("Workflow Builtin Tools Handlers", () => {
  let mockContext: BuiltinToolExecutionContext;

  beforeEach(() => {
    vi.clearAllMocks();

    // Mock context
    mockContext = {
      executionId: "test-thread-123",
      parentThreadEntity: {
        id: "parent-thread-123",
        getThreadId: vi.fn(() => "parent-thread-123"),
        getWorkflowId: vi.fn(() => "workflow-123"),
        getInput: vi.fn(() => ({ key: "value" })),
        getOutput: vi.fn(() => ({ result: "success" })),
        registerChildThread: vi.fn(),
        unregisterChildThread: vi.fn(),
      } as any,
    };
  });

  describe("Execute Workflow Handler", () => {
    it("should execute workflow synchronously with waitForCompletion=true", async () => {
      const handler = createExecuteWorkflowHandler();

      const mockResult = {
        subgraphEntity: {
          getOutput: vi.fn(() => ({ data: "test" })),
        },
        threadResult: {
          executionId: "subgraph-thread-456",
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
      expect(result.output).toEqual({ data: "test" });
      expect(result.executionTime).toBe(100);
      expect(mockTriggeredSubworkflowManager.executeTriggeredSubgraph).toHaveBeenCalledWith(
        expect.objectContaining({
          subgraphId: "test-workflow",
          input: { customInput: "test" },
          mainThreadEntity: mockContext.parentThreadEntity,
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
      expect(result.taskId).toBe("task-123");
      expect(result.message).toBe("Task submitted");
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

    it("should throw error when parentThreadEntity is missing", async () => {
      const handler = createExecuteWorkflowHandler();

      const invalidContext = {
        executionId: "test-thread-123",
        // parentThreadEntity is missing
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

      const mockTaskInfo = {
        id: "task-123",
        status: "RUNNING",
        executionEntity: {
          getThreadId: vi.fn(() => "thread-456"),
          getWorkflowId: vi.fn(() => "workflow-789"),
        },
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      mockTriggeredSubworkflowManager.getTaskStatus.mockReturnValue(mockTaskInfo);

      const result = await handler({ taskId: "task-123" }, mockContext);

      expect(result.success).toBe(true);
      expect(result.status).toBe("RUNNING");
      expect(result.executionId).toBe("thread-456");
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
