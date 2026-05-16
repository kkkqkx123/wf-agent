/**
 * Workflow Execution Adapter Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { WorkflowExecutionAdapter } from "../workflow-execution-adapter.js";
import { SDKInstance } from "@wf-agent/sdk/api";
import { success, failure } from "@wf-agent/sdk/api";

// Mock SDK dependencies
const mockExecutionsApi = {
  getAll: vi.fn(),
  get: vi.fn(),
  delete: vi.fn(),
};

const mockDependencies = {
  getExecutionCoordinator: vi.fn(),
};

const mockFactory = {
  getDependencies: vi.fn(() => mockDependencies),
};

const mockSdk = {
  getFactory: vi.fn(() => mockFactory),
  executeCommand: vi.fn(),
  executions: mockExecutionsApi,
};

vi.mock("@wf-agent/sdk", async () => {
  const actual = await vi.importActual("@wf-agent/sdk");
  return {
    ...actual,
  };
});

vi.mock("../../src/index.js", () => ({
  getSDKInstance: vi.fn(() => mockSdk),
}));

vi.mock("../../src/utils/output.js", () => ({
  getOutput: vi.fn(() => ({
    infoLog: vi.fn(),
    errorLog: vi.fn(),
    success: vi.fn(),
    fail: vi.fn(),
    result: vi.fn(),
    errorResult: vi.fn(),
    debugLog: vi.fn(),
  })),
}));

describe("WorkflowExecutionAdapter", () => {
  let adapter: WorkflowExecutionAdapter;

  beforeEach(() => {
    vi.clearAllMocks();

    // Create adapter instance
    adapter = new WorkflowExecutionAdapter();
  });

  describe("executeWorkflow", () => {
    it("should execute workflow successfully", async () => {
      const mockResult = { executionId: "exec-123", status: "running" };
      mockSdk.executeCommand.mockResolvedValue(success(mockResult, 100));

      const result = await adapter.executeWorkflow("workflow-1", { input: "test" });

      expect(mockSdk.getFactory).toHaveBeenCalled();
      expect(mockSdk.executeCommand).toHaveBeenCalled();
      expect(result).toEqual(mockResult);
    });

    it("should handle execution failure", async () => {
      const mockError = new Error("Execution failed");
      mockSdk.executeCommand.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.executeWorkflow("workflow-1")).rejects.toThrow("Execution failed");
    });
  });

  describe("pauseWorkflowExecution", () => {
    it("should pause workflow execution successfully", async () => {
      mockSdk.executeCommand.mockResolvedValue(success(undefined, 50));

      await adapter.pauseWorkflowExecution("exec-123");

      expect(mockSdk.executeCommand).toHaveBeenCalled();
    });

    it("should handle pause failure", async () => {
      const mockError = new Error("Pause failed");
      mockSdk.executeCommand.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.pauseWorkflowExecution("exec-123")).rejects.toThrow("Pause failed");
    });
  });

  describe("resumeWorkflowExecution", () => {
    it("should resume workflow execution successfully", async () => {
      mockSdk.executeCommand.mockResolvedValue(success(undefined, 50));

      await adapter.resumeWorkflowExecution("exec-123");

      expect(mockSdk.executeCommand).toHaveBeenCalled();
    });

    it("should handle resume failure", async () => {
      const mockError = new Error("Resume failed");
      mockSdk.executeCommand.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.resumeWorkflowExecution("exec-123")).rejects.toThrow("Resume failed");
    });
  });

  describe("stopWorkflowExecution", () => {
    it("should stop workflow execution successfully", async () => {
      mockSdk.executeCommand.mockResolvedValue(success(undefined, 50));

      await adapter.stopWorkflowExecution("exec-123");

      expect(mockSdk.executeCommand).toHaveBeenCalled();
    });

    it("should handle stop failure", async () => {
      const mockError = new Error("Stop failed");
      mockSdk.executeCommand.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.stopWorkflowExecution("exec-123")).rejects.toThrow("Stop failed");
    });
  });

  describe("listWorkflowExecutions", () => {
    it("should list all workflow executions", async () => {
      const mockExecutions = [
        { id: "exec-1", workflowId: "wf-1", status: "completed" },
        { id: "exec-2", workflowId: "wf-2", status: "running" },
      ];
      mockExecutionsApi.getAll.mockResolvedValue(success(mockExecutions, 30));

      const result = await adapter.listWorkflowExecutions();

      expect(mockExecutionsApi.getAll).toHaveBeenCalled();
      expect(result).toHaveLength(2);
      expect(result[0]).toHaveProperty("id", "exec-1");
      expect(result[0]).toHaveProperty("workflowId", "wf-1");
      expect(result[0]).toHaveProperty("status", "completed");
    });

    it("should handle list failure", async () => {
      const mockError = new Error("List failed");
      mockExecutionsApi.getAll.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.listWorkflowExecutions()).rejects.toThrow("List failed");
    });
  });

  describe("getWorkflowExecution", () => {
    it("should get workflow execution details", async () => {
      const mockExecution = { id: "exec-123", workflowId: "wf-1", status: "running" };
      mockExecutionsApi.get.mockResolvedValue(success(mockExecution, 20));

      const result = await adapter.getWorkflowExecution("exec-123");

      expect(mockExecutionsApi.get).toHaveBeenCalledWith("exec-123");
      expect(result).toEqual(mockExecution);
    });

    it("should throw error when execution not found", async () => {
      mockExecutionsApi.get.mockResolvedValue(success(null, 20));

      await expect(adapter.getWorkflowExecution("non-existent")).rejects.toThrow(
        "Workflow execution not found: non-existent",
      );
    });

    it("should handle get failure", async () => {
      const mockError = new Error("Get failed");
      mockExecutionsApi.get.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.getWorkflowExecution("exec-123")).rejects.toThrow("Get failed");
    });
  });

  describe("deleteWorkflowExecution", () => {
    it("should delete workflow execution", async () => {
      mockExecutionsApi.delete.mockResolvedValue(undefined);

      await adapter.deleteWorkflowExecution("exec-123");

      expect(mockExecutionsApi.delete).toHaveBeenCalledWith("exec-123");
    });
  });
});
