import { describe, it, expect, vi, beforeEach } from "vitest";
import { scriptHandler, extractPath } from "../script-handler.js";
import type { GlobalContext } from "../../../../../shared/global-context.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode, ScriptNodeConfig } from "@wf-agent/types";

const mockGlobalContext = {
  container: {
    get: vi.fn(),
  },
} as unknown as GlobalContext;

const mockEntity = {
  getNodeResults: vi.fn().mockReturnValue([]),
  addNodeResult: vi.fn(),
  setVariable: vi.fn(),
  getOutput: vi.fn().mockReturnValue({}),
  setOutput: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockScriptService = {
  execute: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockGlobalContext.container.get as any).mockReturnValue(mockScriptService);
});

describe("scriptHandler", () => {
  describe("basic execution", () => {
    it("should execute script and return result", async () => {
      mockScriptService.execute.mockResolvedValue({
        isErr: () => false,
        value: "script result",
      });

      const config: ScriptNodeConfig = { scriptName: "my-script", risk: "none" };
      const node = { id: "script-node-1", type: "SCRIPT", config } as RuntimeNode;

      const result = await scriptHandler(mockGlobalContext, mockEntity, node);

      expect(mockScriptService.execute).toHaveBeenCalledWith("my-script", {}, mockScriptService);
      expect(result).toBe("script result");
    });

    it("should handle script execution failure", async () => {
      const scriptError = new Error("Script failed");
      mockScriptService.execute.mockResolvedValue({
        isErr: () => true,
        error: scriptError,
      });

      const config: ScriptNodeConfig = { scriptName: "failing-script", risk: "none" };
      const node = { id: "script-node-2", type: "SCRIPT", config } as RuntimeNode;

      await expect(scriptHandler(mockGlobalContext, mockEntity, node)).rejects.toThrow(
        "Script failed",
      );
    });

    it("should handle script service throwing error", async () => {
      mockScriptService.execute.mockRejectedValue(new Error("Service error"));

      const config: ScriptNodeConfig = { scriptName: "error-script", risk: "none" };
      const node = { id: "script-node-3", type: "SCRIPT", config } as RuntimeNode;

      await expect(scriptHandler(mockGlobalContext, mockEntity, node)).rejects.toThrow(
        "Service error",
      );
    });
  });

  describe("outputMapping", () => {
    it("should not apply outputMapping when not configured (backward compatibility)", async () => {
      mockScriptService.execute.mockResolvedValue({
        isErr: () => false,
        value: { data: "test" },
      });

      const config: ScriptNodeConfig = { scriptName: "no-mapping", risk: "none" };
      const node = { id: "script-node-4", type: "SCRIPT", config } as RuntimeNode;

      const result = await scriptHandler(mockGlobalContext, mockEntity, node);

      expect(result).toEqual({ data: "test" });
      expect(mockEntity.setVariable).not.toHaveBeenCalled();
      expect(mockEntity.setOutput).not.toHaveBeenCalled();
    });

    it("should write result to variable with single outputMapping", async () => {
      mockScriptService.execute.mockResolvedValue({
        isErr: () => false,
        value: "computed-value",
      });

      const config: ScriptNodeConfig = {
        scriptName: "compute",
        risk: "none",
        outputMapping: { target: "variable", key: "myVar", description: "test" },
      };
      const node = { id: "script-node-5", type: "SCRIPT", config } as RuntimeNode;

      const result = await scriptHandler(mockGlobalContext, mockEntity, node);

      expect(result).toBe("computed-value");
      expect(mockEntity.setVariable).toHaveBeenCalledWith("myVar", "computed-value");
      expect(mockEntity.setOutput).not.toHaveBeenCalled();
    });

    it("should write result to output with single outputMapping", async () => {
      mockEntity.getOutput = vi.fn().mockReturnValue({ existingKey: "existing" });
      mockScriptService.execute.mockResolvedValue({
        isErr: () => false,
        value: "output-value",
      });

      const config: ScriptNodeConfig = {
        scriptName: "produce-output",
        risk: "none",
        outputMapping: { target: "output", key: "resultKey" },
      };
      const node = { id: "script-node-6", type: "SCRIPT", config } as RuntimeNode;

      const result = await scriptHandler(mockGlobalContext, mockEntity, node);

      expect(result).toBe("output-value");
      expect(mockEntity.setVariable).not.toHaveBeenCalled();
      expect(mockEntity.getOutput).toHaveBeenCalled();
      expect(mockEntity.setOutput).toHaveBeenCalledWith({
        existingKey: "existing",
        resultKey: "output-value",
      });
    });

    it("should handle multiple outputMappings", async () => {
      mockEntity.getOutput = vi.fn().mockReturnValue({});
      mockScriptService.execute.mockResolvedValue({
        isErr: () => false,
        value: { user: "john", score: 95, details: { role: "admin" } },
      });

      const config: ScriptNodeConfig = {
        scriptName: "multi-output",
        risk: "none",
        outputMapping: [
          { target: "variable", key: "userName", path: "user", description: "User name" },
          { target: "variable", key: "userScore", path: "score", description: "Score" },
          { target: "output", key: "processedResult", description: "Final output" },
        ],
      };
      const node = { id: "script-node-7", type: "SCRIPT", config } as RuntimeNode;

      const result = await scriptHandler(mockGlobalContext, mockEntity, node);

      expect(result).toEqual({ user: "john", score: 95, details: { role: "admin" } });
      expect(mockEntity.setVariable).toHaveBeenCalledWith("userName", "john");
      expect(mockEntity.setVariable).toHaveBeenCalledWith("userScore", 95);
      expect(mockEntity.setOutput).toHaveBeenCalledWith({
        processedResult: { user: "john", score: 95, details: { role: "admin" } },
      });
    });

    it("should apply path extraction for nested value", async () => {
      mockScriptService.execute.mockResolvedValue({
        isErr: () => false,
        value: { nested: { deep: { value: "found" } } },
      });

      const config: ScriptNodeConfig = {
        scriptName: "nested-script",
        risk: "none",
        outputMapping: { target: "variable", key: "deepValue", path: "nested.deep.value" },
      };
      const node = { id: "script-node-8", type: "SCRIPT", config } as RuntimeNode;

      const result = await scriptHandler(mockGlobalContext, mockEntity, node);

      expect(result).toEqual({ nested: { deep: { value: "found" } } });
      expect(mockEntity.setVariable).toHaveBeenCalledWith("deepValue", "found");
    });

    it("should return undefined for non-existent path", async () => {
      mockScriptService.execute.mockResolvedValue({
        isErr: () => false,
        value: { a: { b: 1 } },
      });

      const config: ScriptNodeConfig = {
        scriptName: "missing-path",
        risk: "none",
        outputMapping: { target: "variable", key: "missing", path: "a.b.c.d" },
      };
      const node = { id: "script-node-9", type: "SCRIPT", config } as RuntimeNode;

      const result = await scriptHandler(mockGlobalContext, mockEntity, node);

      expect(result).toEqual({ a: { b: 1 } });
      expect(mockEntity.setVariable).toHaveBeenCalledWith("missing", undefined);
    });
  });

  describe("extractPath", () => {
    it("should return obj when path is empty", () => {
      expect(extractPath({ a: 1 }, "")).toEqual({ a: 1 });
    });

    it("should extract top-level key", () => {
      expect(extractPath({ a: 1, b: 2 }, "a")).toBe(1);
    });

    it("should extract nested key with dot notation", () => {
      expect(extractPath({ a: { b: { c: 42 } } }, "a.b.c")).toBe(42);
    });

    it("should return undefined for non-existent key", () => {
      expect(extractPath({ a: 1 }, "b")).toBeUndefined();
    });

    it("should return undefined for non-existent nested path", () => {
      expect(extractPath({ a: { b: 1 } }, "a.x.y")).toBeUndefined();
    });

    it("should handle null values gracefully", () => {
      expect(extractPath(null, "a")).toBeUndefined();
    });

    it("should handle primitive values", () => {
      expect(extractPath(42, "a")).toBeUndefined();
    });

    it("should handle array index access", () => {
      expect(extractPath([1, { x: 10 }, 3], "1.x")).toBe(10);
    });
  });
});