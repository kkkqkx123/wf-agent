/**
 * Tests for TaskExecutor
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { TerminalSession, TaskStatus } from "../types.js";
import type { SDKInstance } from "@wf-agent/sdk";

// Mock output
vi.mock("../../utils/output.js", () => ({
  getOutput: vi.fn(() => ({
    debugLog: vi.fn(),
    infoLog: vi.fn(),
    errorLog: vi.fn(),
    warnLog: vi.fn(),
  })),
}));

// Import after mocking
import { TaskExecutor } from "../task-executor.js";

describe("TaskExecutor", () => {
  let executor: TaskExecutor;
  let mockTerminal: TerminalSession;
  let mockSDK: SDKInstance;

  beforeEach(() => {
    // Create a mock SDK instance
    mockSDK = {} as SDKInstance;
    
    // Pass mock SDK to avoid importing index.js
    executor = new TaskExecutor(mockSDK);
    
    // Create a mock terminal session
    const mockPty = {
      pid: 12345,
      write: vi.fn(),
      onData: vi.fn(),
      onExit: vi.fn(),
      resize: vi.fn(),
      kill: vi.fn(),
    };

    mockTerminal = {
      id: "test-terminal-id",
      pty: mockPty as any,
      pid: 12345,
      createdAt: new Date(),
      status: "active",
    };
  });

  describe("executeInTerminal", () => {
    it("should execute task in terminal", async () => {
      const workflowId = "workflow-123";
      const input = { param1: "value1" };

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);

      expect(result).toBeDefined();
      expect(result.taskId).toBeDefined();
      expect(result.sessionId).toBe(mockTerminal.id);
      expect(result.status).toBe("started");
      expect(result.startTime).toBeInstanceOf(Date);
    });

    it("should throw error for background terminals", async () => {
      const workflowId = "workflow-123";
      const input = {};
      
      // Create a background terminal (ChildProcess instead of IPty)
      const backgroundTerminal: TerminalSession = {
        id: "background-terminal",
        pty: {} as any, // Not an IPty
        pid: 12346,
        createdAt: new Date(),
        status: "active",
      };

      await expect(executor.executeInTerminal(workflowId, input, backgroundTerminal)).rejects.toThrow(
        "Write operation is not supported for background terminals"
      );
    });

    it("should update task status on failure", async () => {
      const workflowId = "workflow-123";
      const input = {};
      
      const backgroundTerminal: TerminalSession = {
        id: "background-terminal",
        pty: {} as any,
        pid: 12346,
        createdAt: new Date(),
        status: "active",
      };

      try {
        await executor.executeInTerminal(workflowId, input, backgroundTerminal);
      } catch (error) {
        // Expected to fail
      }

      const tasks = executor.getAllTasks();
      expect(tasks.length).toBeGreaterThan(0);
    });
  });

  describe("monitorTask", () => {
    it("should monitor existing task", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      const status = await executor.monitorTask(result.taskId);

      expect(status).toBeDefined();
      expect(status.taskId).toBe(result.taskId);
      expect(status.status).toBe("running");
    });

    it("should throw error for non-existent task", async () => {
      await expect(executor.monitorTask("non-existent-task")).rejects.toThrow(
        "Task not found: non-existent-task"
      );
    });
  });

  describe("stopTask", () => {
    it("should stop running task", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      await executor.stopTask(result.taskId);
      
      const status = await executor.monitorTask(result.taskId);
      expect(status.status).toBe("cancelled");
    });

    it("should throw error for non-existent task", async () => {
      await expect(executor.stopTask("non-existent-task")).rejects.toThrow(
        "Task not found: non-existent-task"
      );
    });

    it("should throw error for already completed task", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      executor.markTaskCompleted(result.taskId, "Done");

      await expect(executor.stopTask(result.taskId)).rejects.toThrow(
        "Task cannot be stopped"
      );
    });

    it("should throw error for already failed task", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      executor.markTaskFailed(result.taskId, "Error occurred");

      await expect(executor.stopTask(result.taskId)).rejects.toThrow(
        "Task cannot be stopped"
      );
    });

    it("should throw error for already cancelled task", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      await executor.stopTask(result.taskId);

      await expect(executor.stopTask(result.taskId)).rejects.toThrow(
        "Task cannot be stopped"
      );
    });
  });

  describe("updateTaskStatus", () => {
    it("should update task status", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      executor.updateTaskStatus(result.taskId, {
        status: "running",
        progress: 50,
        message: "Halfway done",
      });

      const status = await executor.monitorTask(result.taskId);
      expect(status.progress).toBe(50);
      expect(status.message).toBe("Halfway done");
    });

    it("should not update non-existent task", () => {
      // Should not throw
      expect(() => {
        executor.updateTaskStatus("non-existent", { status: "running" });
      }).not.toThrow();
    });
  });

  describe("getAllTasks and getActiveTasks", () => {
    it("should get all tasks", async () => {
      const workflowId = "workflow-123";
      const input = {};

      await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      const tasks = executor.getAllTasks();
      expect(tasks.length).toBeGreaterThan(0);
    });

    it("should get active tasks", async () => {
      const workflowId = "workflow-123";
      const input = {};

      await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      const activeTasks = executor.getActiveTasks();
      expect(activeTasks.length).toBeGreaterThan(0);
      expect(activeTasks[0].status).toBe("running");
    });

    it("should filter out inactive tasks", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      executor.markTaskCompleted(result.taskId);
      
      const activeTasks = executor.getActiveTasks();
      expect(activeTasks.length).toBe(0);
    });
  });

  describe("getTerminalId", () => {
    it("should get terminal ID for task", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      const terminalId = executor.getTerminalId(result.taskId);
      expect(terminalId).toBe(mockTerminal.id);
    });

    it("should return undefined for non-existent task", () => {
      const terminalId = executor.getTerminalId("non-existent");
      expect(terminalId).toBeUndefined();
    });
  });

  describe("cleanupOldTasks", () => {
    it("should cleanup old completed tasks", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      executor.markTaskCompleted(result.taskId);
      
      // Set lastUpdate to 2 hours ago
      const tasks = executor.getAllTasks();
      if (tasks.length > 0) {
        tasks[0].lastUpdate = new Date(Date.now() - 2 * 60 * 60 * 1000);
      }
      
      executor.cleanupOldTasks(3600000); // 1 hour
      
      const remainingTasks = executor.getAllTasks();
      expect(remainingTasks.length).toBe(0);
    });

    it("should not cleanup recent tasks", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      executor.markTaskCompleted(result.taskId);
      
      executor.cleanupOldTasks(3600000); // 1 hour
      
      const remainingTasks = executor.getAllTasks();
      expect(remainingTasks.length).toBe(1);
    });

    it("should not cleanup running tasks", async () => {
      const workflowId = "workflow-123";
      const input = {};

      await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      // Set lastUpdate to 2 hours ago
      const tasks = executor.getAllTasks();
      if (tasks.length > 0) {
        tasks[0].lastUpdate = new Date(Date.now() - 2 * 60 * 60 * 1000);
      }
      
      executor.cleanupOldTasks(3600000); // 1 hour
      
      const remainingTasks = executor.getAllTasks();
      expect(remainingTasks.length).toBe(1);
    });
  });

  describe("markTaskCompleted", () => {
    it("should mark task as completed", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      executor.markTaskCompleted(result.taskId, "All done");
      
      const status = await executor.monitorTask(result.taskId);
      expect(status.status).toBe("completed");
      expect(status.progress).toBe(100);
      expect(status.message).toBe("All done");
    });

    it("should use default message if not provided", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      executor.markTaskCompleted(result.taskId);
      
      const status = await executor.monitorTask(result.taskId);
      expect(status.message).toBe("Task completed");
    });
  });

  describe("markTaskFailed", () => {
    it("should mark task as failed", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      executor.markTaskFailed(result.taskId, "Something went wrong");
      
      const status = await executor.monitorTask(result.taskId);
      expect(status.status).toBe("failed");
      expect(status.message).toContain("Something went wrong");
    });
  });

  describe("updateTaskProgress", () => {
    it("should update task progress", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      executor.updateTaskProgress(result.taskId, 75, "Almost there");
      
      const status = await executor.monitorTask(result.taskId);
      expect(status.progress).toBe(75);
      expect(status.message).toBe("Almost there");
    });

    it("should clamp progress to 0-100 range", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      executor.updateTaskProgress(result.taskId, 150);
      const status1 = await executor.monitorTask(result.taskId);
      expect(status1.progress).toBe(100);
      
      executor.updateTaskProgress(result.taskId, -50);
      const status2 = await executor.monitorTask(result.taskId);
      expect(status2.progress).toBe(0);
    });

    it("should use default message if not provided", async () => {
      const workflowId = "workflow-123";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      executor.updateTaskProgress(result.taskId, 50);
      
      const status = await executor.monitorTask(result.taskId);
      expect(status.message).toBe("Progress: 50%");
    });
  });

  describe("buildExecutionCommand", () => {
    it("should build command with input", async () => {
      const workflowId = "workflow-123";
      const input = { param1: "value1", param2: "value2" };

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      // The command should include the workflow ID and input
      expect(result.taskId).toBeDefined();
    });

    it("should build command without input", async () => {
      const workflowId = "workflow-456";
      const input = {};

      const result = await executor.executeInTerminal(workflowId, input, mockTerminal);
      
      expect(result.taskId).toBeDefined();
    });
  });
});
