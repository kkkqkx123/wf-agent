import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import { pauseExecutionHandler } from "../pause-execution-handler.js";
import type { WorkflowExecutionRegistry } from "../../../../stores/workflow-execution-registry.js";

const mockEntity = {
  id: "exec-1",
  pause: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockRegistry = {
  get: vi.fn(),
} as unknown as WorkflowExecutionRegistry;

describe("pause-execution-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should pause workflow execution successfully", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    const action: TriggerAction = {
      type: "pause_workflow_execution",
      parameters: { executionId: "exec-1" },
    };

    const result = await pauseExecutionHandler(action, "trigger-1", mockRegistry);

    expect(mockRegistry.get).toHaveBeenCalledWith("exec-1");
    expect(mockEntity.pause).toHaveBeenCalled();
    expect(result.success).toBe(true);
    expect(result.triggerId).toBe("trigger-1");
    expect(result.action).toEqual(action);
  });

  it("should throw RuntimeValidationError when executionId is missing", async () => {
    const action = {
      type: "pause_workflow_execution",
      parameters: {},
    } as unknown as TriggerAction;

    const result = await pauseExecutionHandler(action, "trigger-2", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("executionId is required");
    expect(mockRegistry.get).not.toHaveBeenCalled();
  });

  it("should throw WorkflowExecutionNotFoundError when execution not found", async () => {
    (mockRegistry.get as any).mockReturnValue(null);
    const action: TriggerAction = {
      type: "pause_workflow_execution",
      parameters: { executionId: "exec-not-found" },
    };

    const result = await pauseExecutionHandler(action, "trigger-3", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
    expect(mockRegistry.get).toHaveBeenCalledWith("exec-not-found");
  });

  it("should handle other errors gracefully", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    (mockEntity.pause as any).mockImplementation(() => {
      throw new Error("Some unexpected error");
    });
    const action: TriggerAction = {
      type: "pause_workflow_execution",
      parameters: { executionId: "exec-1" },
    };

    const result = await pauseExecutionHandler(action, "trigger-4", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("Some unexpected error");
  });
});