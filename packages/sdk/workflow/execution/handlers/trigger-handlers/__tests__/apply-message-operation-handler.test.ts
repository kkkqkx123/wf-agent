import { describe, it, expect, vi, beforeEach } from "vitest";
import type { TriggerAction } from "@wf-agent/types";
import { applyMessageOperationHandler } from "../apply-message-operation-handler.js";
import type { WorkflowExecutionRegistry } from "../../../../registry/workflow-execution-registry.js";
import type { WorkflowStateCoordinator } from "../../../../state-managers/workflow-state-coordinator.js";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";

const mockConversationManager = {
  executeMessageOperation: vi.fn(),
};

const mockEntity = {
  id: "exec-1",
} as unknown as WorkflowExecutionEntity;

const mockRegistry = {
  get: vi.fn(),
} as unknown as WorkflowExecutionRegistry;

const mockCoordinator = {
  getConversationManager: vi.fn(),
} as unknown as WorkflowStateCoordinator;

const mockCoordinatorMap = new Map<string, WorkflowStateCoordinator>();

describe("apply-message-operation-handler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCoordinatorMap.clear();
  });

  it("should apply message operation successfully", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    mockCoordinatorMap.set("exec-1", mockCoordinator);
    (mockCoordinator.getConversationManager as any).mockReturnValue(mockConversationManager);
    (mockConversationManager.executeMessageOperation as any).mockResolvedValue({
      stats: { messagesAdded: 2, tokensUsed: 100 },
    });

    const action = {
      type: "apply_message_operation",
      parameters: {
        executionId: "exec-1",
        operationConfig: {
          operation: "add_message",
          message: { role: "user", content: "Hello" },
        },
      } as any,
    } as unknown as TriggerAction;

    const result = await applyMessageOperationHandler(
      action,
      "trigger-1",
      mockRegistry,
      mockCoordinatorMap,
    );

    expect(mockRegistry.get).toHaveBeenCalledWith("exec-1");
    expect(mockCoordinator.getConversationManager).toHaveBeenCalled();
    expect(mockConversationManager.executeMessageOperation).toHaveBeenCalledWith({
      operation: "add_message",
      message: { role: "user", content: "Hello" },
    });
    expect(result.success).toBe(true);
    expect(result.result).toEqual({
      message: "Message operation add_message applied successfully to workflow execution exec-1",
      stats: { messagesAdded: 2, tokensUsed: 100 },
    });
  });

  it("should fail when executionId is missing", async () => {
    const action = {
      type: "apply_message_operation",
      parameters: {
        operationConfig: { operation: "clear" },
      } as any,
    } as unknown as TriggerAction;

    const result = await applyMessageOperationHandler(
      action,
      "trigger-2",
      mockRegistry,
      mockCoordinatorMap,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("executionId is required");
  });

  it("should fail when operationConfig is missing", async () => {
    const action = {
      type: "apply_message_operation",
      parameters: { executionId: "exec-1" } as any,
    } as unknown as TriggerAction;

    const result = await applyMessageOperationHandler(
      action,
      "trigger-3",
      mockRegistry,
      mockCoordinatorMap,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("operationConfig is required");
  });

  it("should fail when workflow execution not found", async () => {
    (mockRegistry.get as any).mockReturnValue(null);
    const action = {
      type: "apply_message_operation",
      parameters: {
        executionId: "exec-not-found",
        operationConfig: { operation: "clear" },
      } as any,
    } as unknown as TriggerAction;

    const result = await applyMessageOperationHandler(
      action,
      "trigger-4",
      mockRegistry,
      mockCoordinatorMap,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });

  it("should fail when state coordinator not found", async () => {
    (mockRegistry.get as any).mockReturnValue(mockEntity);
    const action = {
      type: "apply_message_operation",
      parameters: {
        executionId: "exec-no-coord",
        operationConfig: { operation: "clear" },
      } as any,
    } as unknown as TriggerAction;

    const result = await applyMessageOperationHandler(
      action,
      "trigger-5",
      mockRegistry,
      mockCoordinatorMap,
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain("not found");
  });
});
