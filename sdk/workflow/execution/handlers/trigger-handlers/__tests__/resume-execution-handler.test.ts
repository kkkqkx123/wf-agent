import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction, WorkflowExecutionEntity } from "@wf-agent/types";
import { resumeExecutionHandler } from "../resume-execution-handler.js";
import type { WorkflowExecutionRegistry } from "../../../../stores/workflow-execution-registry.js";

const mockEntity = {
  id: "exec-1",
  resume: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockRegistry = {
  get: vi.fn(),
} as unknown as WorkflowExecutionRegistry;

describe("resume-execution-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should resume workflow execution successfully", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    const action: TriggerAction = {
      type: "resume_workflow_execution",
      parameters: { executionId: "exec-1" },
    };

    const result = await resumeExecutionHandler(action, "trigger-1", mockRegistry);

    expect(mockRegistry.get).toHaveBeenCalledWith("exec-1");
    expect(mockEntity.resume).toHaveBeenCalled();
    expect(result.success).toBe(true);
    expect(result.triggerId).toBe("trigger-1");
  });

  it("should fail when executionId is missing", async () => {
    const action: TriggerAction = {
      type: "resume_workflow_execution",
      parameters: {},
    };

    const result = await resumeExecutionHandler(action, "trigger-2", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("executionId is required");
  });

  it("should fail when execution not found", async () => {
    (mockRegistry.get as any).mockReturnValue(null);
    const action: TriggerAction = {
      type: "resume_workflow_execution",
      parameters: { executionId: "exec-not-found" },
    };

    const result = await resumeExecutionHandler(action, "trigger-3", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });

  it("should handle unexpected errors", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    (mockEntity.resume as any).mockImplementation(() => {
      throw new Error("Unexpected resume error");
    });
    const action: TriggerAction = {
      type: "resume_workflow_execution",
      parameters: { executionId: "exec-1" },
    };

    const result = await resumeExecutionHandler(action, "trigger-4", mockRegistry);

    expect(result.success).toBe(false);
    expect(result.error).toContain("Unexpected resume error");
  });
});