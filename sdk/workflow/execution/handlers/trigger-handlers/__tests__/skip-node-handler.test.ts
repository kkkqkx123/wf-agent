import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import { skipNodeHandler } from "../skip-node-handler.js";
import type { WorkflowExecutionRegistry } from "../../../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../../../core/registry/event-registry.js";

const mockExecution = {
  nodeResults: [] as Array<{
    nodeId: string;
    nodeType: string;
    status: string;
    step: number;
    executionTime: number;
  }>,
};

const mockEntity = {
  id: "exec-1",
  getWorkflowId: vi.fn().mockReturnValue("wf-1"),
  getWorkflowExecutionData: vi.fn().mockReturnValue(mockExecution),
} as unknown as WorkflowExecutionEntity;

const mockRegistry = {
  get: vi.fn(),
} as unknown as WorkflowExecutionRegistry;

const mockEventManager = {
  emit: vi.fn(),
} as unknown as EventRegistry;

describe("skip-node-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockExecution.nodeResults = [];
  });

  it("should skip node successfully", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    const action: TriggerAction = {
      type: "skip_node",
      parameters: { executionId: "exec-1", nodeId: "node-1" },
    };

    const result = await skipNodeHandler(action, "trigger-1", mockRegistry, mockEventManager);

    expect(mockRegistry.get).toHaveBeenCalledWith("exec-1");
    expect(mockExecution.nodeResults).toHaveLength(1);
    expect(mockExecution.nodeResults[0]).toEqual({
      nodeId: "node-1",
      nodeType: "UNKNOWN",
      status: "SKIPPED",
      step: 1,
      executionTime: 0,
    });
    expect(mockEventManager.emit).toHaveBeenCalled();
    expect(result.success).toBe(true);
    expect(result.result).toEqual({
      message: "Node node-1 skipped successfully in workflow execution exec-1",
    });
  });

  it("should skip node and increment step based on existing results", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    mockExecution.nodeResults = [
      { nodeId: "node-0", nodeType: "START", status: "COMPLETED", step: 1, executionTime: 10 },
    ];
    const action: TriggerAction = {
      type: "skip_node",
      parameters: { executionId: "exec-1", nodeId: "node-2" },
    };

    await skipNodeHandler(action, "trigger-2", mockRegistry, mockEventManager);

    expect(mockExecution.nodeResults).toHaveLength(2);
    expect(mockExecution.nodeResults[1]!.step).toBe(2);
  });

  it("should fail when executionId is missing", async () => {
    const action = {
      type: "skip_node",
      parameters: { nodeId: "node-1" } as any,
    } as unknown as TriggerAction;

    const result = await skipNodeHandler(action, "trigger-3", mockRegistry, mockEventManager);

    expect(result.success).toBe(false);
    expect(result.error).toContain("executionId is required");
  });

  it("should fail when nodeId is missing", async () => {
    const action = {
      type: "skip_node",
      parameters: { executionId: "exec-1" } as any,
    } as unknown as TriggerAction;

    const result = await skipNodeHandler(action, "trigger-4", mockRegistry, mockEventManager);

    expect(result.success).toBe(false);
    expect(result.error).toContain("nodeId is required");
  });

  it("should fail when execution not found", async () => {
    (mockRegistry.get as any).mockReturnValue(null);
    const action: TriggerAction = {
      type: "skip_node",
      parameters: { executionId: "exec-not-found", nodeId: "node-1" },
    };

    const result = await skipNodeHandler(action, "trigger-5", mockRegistry, mockEventManager);

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });
});
