import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import { setVariableHandler } from "../set-variable-handler.js";
import type { WorkflowExecutionRegistry } from "../../../../stores/workflow-execution-registry.js";

const mockEntity = {
  id: "exec-1",
  getExecution: vi.fn(),
  setVariable: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockRegistry = {
  get: vi.fn(),
} as unknown as WorkflowExecutionRegistry;

describe("set-variable-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should set variables successfully", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    const action: TriggerAction = {
      type: "set_variable",
      parameters: {
        executionId: "exec-1",
        variables: { name: "John", age: 30 },
      },
    };

    const result = await setVariableHandler(action, "trigger-1", mockRegistry);

    expect(mockRegistry.get).toHaveBeenCalledWith("exec-1");
    expect(mockEntity.setVariable).toHaveBeenCalledWith("name", "John");
    expect(mockEntity.setVariable).toHaveBeenCalledWith("age", 30);
    expect(result.success).toBe(true);
    expect(result.result).toEqual({
      message: "Variables updated successfully in workflow execution exec-1",
      variables: { name: "John", age: 30 },
    });
  });

  it("should fail when executionId is missing", async () => {
    const action = {
      type: "set_variable",
      parameters: {},
    } as unknown as TriggerAction;

    const result = await setVariableHandler(action, "trigger-2", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("executionId is required");
  });

  it("should fail when variables are missing", async () => {
    const action = {
      type: "set_variable",
      parameters: { executionId: "exec-1" } as any,
    } as unknown as TriggerAction;

    const result = await setVariableHandler(action, "trigger-3", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("variables is required");
  });

  it("should fail when execution not found", async () => {
    (mockRegistry.get as any).mockReturnValue(null);
    const action: TriggerAction = {
      type: "set_variable",
      parameters: {
        executionId: "exec-not-found",
        variables: { name: "John" },
      },
    };

    const result = await setVariableHandler(action, "trigger-4", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });
});
