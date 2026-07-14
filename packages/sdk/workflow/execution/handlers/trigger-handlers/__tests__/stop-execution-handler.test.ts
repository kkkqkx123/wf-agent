import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import { stopExecutionHandler } from "../stop-execution-handler.js";
import type { WorkflowExecutionRegistry } from "../../../../registry/workflow-execution-registry.js";

const mockEntity = {
  id: "exec-1",
  stop: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockRegistry = {
  get: vi.fn(),
} as unknown as WorkflowExecutionRegistry;

describe("stop-execution-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should stop workflow execution successfully", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    const action: TriggerAction = {
      type: "stop_workflow_execution",
      parameters: { executionId: "exec-1" },
    };

    const result = await stopExecutionHandler(action, "trigger-1", mockRegistry);

    expect(mockRegistry.get).toHaveBeenCalledWith("exec-1");
    expect(mockEntity.stop).toHaveBeenCalled();
    expect(result.success).toBe(true);
    expect(result.triggerId).toBe("trigger-1");
  });

  it("should fail when executionId is missing", async () => {
    const action = {
      type: "stop_workflow_execution",
      parameters: {},
    } as unknown as TriggerAction;

    const result = await stopExecutionHandler(action, "trigger-2", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("executionId is required");
  });

  it("should fail when execution not found", async () => {
    (mockRegistry.get as any).mockReturnValue(null);
    const action: TriggerAction = {
      type: "stop_workflow_execution",
      parameters: { executionId: "exec-not-found" },
    };

    const result = await stopExecutionHandler(action, "trigger-3", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });

  it("should handle unexpected errors", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    (mockEntity.stop as any).mockImplementation(() => {
      throw new Error("Stop failed");
    });
    const action: TriggerAction = {
      type: "stop_workflow_execution",
      parameters: { executionId: "exec-1" },
    };

    const result = await stopExecutionHandler(action, "trigger-4", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("Stop failed");
  });
});
