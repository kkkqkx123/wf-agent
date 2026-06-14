import { describe, it, expect, vi, beforeEach } from "vitest";
import { continueFromTriggerHandler } from "../continue-from-trigger-handler.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { RuntimeNode, ContinueFromTriggerNodeConfig } from "@wf-agent/types";
import type { ContinueFromTriggerHandlerContext } from "../continue-from-trigger-handler.js";

const mockMainEntity = {
  getWorkflowExecutionData: vi.fn(),
  setVariable: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockMainExecution = {};

const mockSubEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  getWorkflowExecutionData: vi.fn(),
  addNodeResult: vi.fn(),
  getWorkflowId: vi.fn().mockReturnValue("sub-wf"),
  getOutput: vi.fn().mockReturnValue({}),
  setOutput: vi.fn(),
  variableStateManager: {
    getVariable: vi.fn(),
  },
} as unknown as WorkflowExecutionEntity;

const mockSubExecution = {};

const mockSubRegistry = {
  get: vi.fn(),
  has: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  getAll: vi.fn(),
};

const mockParentRegistry = {
  get: vi.fn(),
  has: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  getAll: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockSubEntity.getStatus as any).mockReturnValue("RUNNING");
  (mockSubEntity.getWorkflowExecutionData as any).mockReturnValue(mockSubExecution);
  (mockMainEntity.getWorkflowExecutionData as any).mockReturnValue(mockMainExecution);
  (mockSubEntity.getNodeResults as any).mockReturnValue([]);
});

describe("continueFromTriggerHandler", () => {
  it("should copy message contexts to parent", async () => {
    const context: ContinueFromTriggerHandlerContext = {
      mainWorkflowExecutionEntity: mockMainEntity,
    };
    const config: ContinueFromTriggerNodeConfig = {
      messageOutputs: [{ internalName: "subCtx", externalName: "parentCtx" }],
    };
    const node = { id: "continue-trigger-1", type: "CONTINUE_FROM_TRIGGER", config } as RuntimeNode;

    (mockSubExecution as any).messageContextRegistry = mockSubRegistry;
    (mockMainExecution as any).messageContextRegistry = mockParentRegistry;
    mockSubRegistry.get.mockReturnValue({
      id: "subCtx",
      messages: [{ role: "assistant", content: "result" }],
      metadata: { source: "test" },
      createdAt: Date.now(),
      updatedAt: Date.now(),
    });

    await continueFromTriggerHandler(mockSubEntity, node, context);

    expect(mockParentRegistry.register).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "parentCtx",
        messages: [{ role: "assistant", content: "result" }],
      }),
    );
  });

  it("should copy variables to parent when configured", async () => {
    const context: ContinueFromTriggerHandlerContext = {
      mainWorkflowExecutionEntity: mockMainEntity,
    };
    const config: ContinueFromTriggerNodeConfig = {
      variableOutputs: [{ internalName: "childResult", externalName: "parentResult" }],
    };
    const node = { id: "continue-trigger-2", type: "CONTINUE_FROM_TRIGGER", config } as RuntimeNode;

    (mockSubEntity.variableStateManager.getVariable as any).mockReturnValue("some-value");

    await continueFromTriggerHandler(mockSubEntity, node, context);

    expect(mockMainEntity.setVariable).toHaveBeenCalledWith("parentResult", "some-value");
  });

  it("should throw when main workflow execution entity is missing", async () => {
    const node = {
      id: "continue-trigger-3",
      type: "CONTINUE_FROM_TRIGGER",
      config: {},
    } as RuntimeNode;

    await expect(continueFromTriggerHandler(mockSubEntity, node, {})).rejects.toThrow(
      "Main workflow execution entity is required for CONTINUE_FROM_TRIGGER node",
    );
  });
});
