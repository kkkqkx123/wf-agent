import { describe, it, expect } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import { createSuccessResult, createFailureResult } from "../trigger-handler-utils.js";

describe("trigger-handler-utils", () => {
  describe("createSuccessResult", () => {
    it("should create a success result with all fields", () => {
      const action: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: { executionId: "exec-1" },
      };
      const data = { message: "Paused successfully" };

      const result = createSuccessResult("trigger-1", action, data, 150);

      expect(result).toEqual({
        triggerId: "trigger-1",
        success: true,
        action,
        executionTime: 150,
        result: data,
      });
    });

    it("should create a success result with undefined data", () => {
      const action: TriggerAction = {
        type: "stop_workflow_execution",
        parameters: { executionId: "exec-1" },
      };

      const result = createSuccessResult("trigger-2", action, undefined, 50);

      expect(result).toEqual({
        triggerId: "trigger-2",
        success: true,
        action,
        executionTime: 50,
        result: undefined,
      });
    });
  });

  describe("createFailureResult", () => {
    it("should create a failure result with error message", () => {
      const action: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: { executionId: "exec-1" },
      };
      const error = new Error("Execution not found");

      const result = createFailureResult("trigger-3", action, error, 200);

      expect(result).toEqual({
        triggerId: "trigger-3",
        success: false,
        action,
        executionTime: 200,
        error: "Execution not found",
      });
    });

    it("should extract error message from non-Error values", () => {
      const action: TriggerAction = {
        type: "stop_workflow_execution",
        parameters: { executionId: "exec-1" },
      };

      const result = createFailureResult("trigger-4", action, "string error", 100);

      expect(result).toEqual({
        triggerId: "trigger-4",
        success: false,
        action,
        executionTime: 100,
        error: "string error",
      });
    });

    it("should handle null error", () => {
      const action: TriggerAction = {
        type: "pause_workflow_execution",
        parameters: { executionId: "exec-1" },
      };

      const result = createFailureResult("trigger-5", action, null, 10);

      expect(result.error).toBe("Unknown error");
    });
  });
});
