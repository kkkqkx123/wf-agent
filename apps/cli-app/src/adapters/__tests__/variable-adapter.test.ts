/**
 * Variable Adapter Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { VariableAdapter } from "../variable-adapter.js";
import { success, failure } from "@wf-agent/sdk/api";

// Mock SDK dependencies
vi.mock("@wf-agent/sdk", async () => {
  const actual = await vi.importActual("@wf-agent/sdk");
  return {
    ...actual,
  };
});

vi.mock("../../src/services/sdk-globals.js", () => ({
  getSDKInstance: vi.fn(),
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

describe("VariableAdapter", () => {
  let adapter: VariableAdapter;
  let mockSdk: any;
  let mockVariablesApi: any;

  beforeEach(() => {
    vi.clearAllMocks();

    // Mock variables API
    mockVariablesApi = {
      get: vi.fn(),
      getAll: vi.fn(),
      setVariable: vi.fn(),
      deleteVariable: vi.fn(),
      getWorkflowExecutionVariableDefinitions: vi.fn(),
    };

    // Mock SDK instance
    mockSdk = {
      variables: mockVariablesApi,
    };

    // Setup getSDKInstance mock
    const { getSDKInstance } = require("../../src/services/sdk-globals.js");
    getSDKInstance.mockReturnValue(mockSdk);

    // Create adapter instance
    adapter = new VariableAdapter();
  });

  describe("getVariable", () => {
    it("should get variable value successfully", async () => {
      const mockValue = { name: "testVar", value: "testValue" };
      mockVariablesApi.get.mockResolvedValue(success(mockValue, 10));

      const result = await adapter.getVariable("exec-123", "testVar");

      expect(mockVariablesApi.get).toHaveBeenCalledWith("exec-123:testVar");
      expect(result).toEqual(mockValue);
    });

    it("should handle get failure", async () => {
      const mockError = new Error("Variable not found");
      mockVariablesApi.get.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.getVariable("exec-123", "testVar")).rejects.toThrow(
        "Variable not found",
      );
    });
  });

  describe("setVariable", () => {
    it("should set variable value successfully", async () => {
      mockVariablesApi.setVariable.mockResolvedValue(success(undefined, 15));

      await adapter.setVariable("exec-123", "testVar", "newValue");

      expect(mockVariablesApi.setVariable).toHaveBeenCalledWith(
        "exec-123",
        "testVar",
        "newValue",
      );
    });

    it("should handle set failure", async () => {
      const mockError = new Error("Failed to set variable");
      mockVariablesApi.setVariable.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.setVariable("exec-123", "testVar", "value")).rejects.toThrow(
        "Failed to set variable",
      );
    });
  });

  describe("listVariables", () => {
    it("should list all variables for an execution", async () => {
      const mockVariables = {
        var1: "value1",
        var2: "value2",
        var3: 123,
      };
      mockVariablesApi.getAll.mockResolvedValue(success(mockVariables, 20));

      const result = await adapter.listVariables("exec-123");

      expect(mockVariablesApi.getAll).toHaveBeenCalledWith({ executionId: "exec-123" });
      expect(result).toEqual(mockVariables);
      expect(Object.keys(result)).toHaveLength(3);
    });

    it("should handle list failure", async () => {
      const mockError = new Error("Failed to list variables");
      mockVariablesApi.getAll.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.listVariables("exec-123")).rejects.toThrow(
        "Failed to list variables",
      );
    });
  });

  describe("deleteVariable", () => {
    it("should delete variable successfully", async () => {
      mockVariablesApi.deleteVariable.mockResolvedValue(success(undefined, 15));

      await adapter.deleteVariable("exec-123", "testVar");

      expect(mockVariablesApi.deleteVariable).toHaveBeenCalledWith("exec-123", "testVar");
    });

    it("should handle delete failure", async () => {
      const mockError = new Error("Failed to delete variable");
      mockVariablesApi.deleteVariable.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.deleteVariable("exec-123", "testVar")).rejects.toThrow(
        "Failed to delete variable",
      );
    });
  });

  describe("getVariableDefinition", () => {
    it("should get variable definition when exists", async () => {
      const mockDefinitions = {
        testVar: {
          name: "testVar",
          type: "string",
          description: "A test variable",
          defaultValue: "default",
          required: true,
        },
      };
      mockVariablesApi.getWorkflowExecutionVariableDefinitions.mockResolvedValue(
        mockDefinitions,
      );

      const result = await adapter.getVariableDefinition("exec-123", "testVar");

      expect(mockVariablesApi.getWorkflowExecutionVariableDefinitions).toHaveBeenCalled();
      expect(result).toEqual(mockDefinitions.testVar);
      expect(result?.name).toBe("testVar");
      expect(result?.type).toBe("string");
    });

    it("should return null when definition not found", async () => {
      const mockDefinitions = {
        otherVar: {
          name: "otherVar",
          type: "string",
        },
      };
      mockVariablesApi.getWorkflowExecutionVariableDefinitions.mockResolvedValue(
        mockDefinitions,
      );

      const result = await adapter.getVariableDefinition("exec-123", "nonExistent");

      expect(result).toBeNull();
    });

    it("should handle errors gracefully", async () => {
      const mockError = new Error("Failed to get definitions");
      mockVariablesApi.getWorkflowExecutionVariableDefinitions.mockRejectedValue(mockError);

      await expect(adapter.getVariableDefinition("exec-123", "testVar")).rejects.toThrow();
    });
  });
});
