import { describe, it, expect, vi, beforeEach } from "vitest";
import { userInteractionHandler } from "../user-interaction-handler.js";
import type { WorkflowExecution } from "@wf-agent/types";
import type { RuntimeNode, UserInteractionNodeConfig } from "@wf-agent/types";
import type { UserInteractionHandlerContext } from "../user-interaction-handler.js";

const mockConversationManager = {
  addMessage: vi.fn(),
};

const mockUserInteractionHandler = {
  handle: vi.fn(),
};

const defaultContext: UserInteractionHandlerContext = {
  userInteractionHandler: mockUserInteractionHandler as any,
  conversationManager: mockConversationManager,
  timeout: 5000,
};

const mockExecution = {
  id: "exec-1",
  workflowId: "wf-1",
  variableScopes: {
    execution: {},
  },
} as unknown as WorkflowExecution;

beforeEach(() => {
  vi.clearAllMocks();
  mockUserInteractionHandler.handle.mockResolvedValue("user input data");
});

describe("userInteractionHandler", () => {
  it("should get user input and update variables for UPDATE_VARIABLES operation", async () => {
    const config: UserInteractionNodeConfig = {
      operationType: "UPDATE_VARIABLES",
      prompt: "Enter value:",
      variables: [{ variableName: "userName", expression: "{{input}}" }],
      timeout: 5000,
    };
    const node = { id: "ui-node-1", type: "USER_INTERACTION", config } as RuntimeNode;

    const result = await userInteractionHandler(mockExecution, node, defaultContext);

    expect(result.operationType).toBe("UPDATE_VARIABLES");
    expect(result.userInput).toBe("user input data");
    expect(result.updatedVariables).toEqual([
      { variableName: "userName", newValue: "user input data" },
    ]);
  });

  it("should get user input and add message for ADD_MESSAGE operation", async () => {
    const config: UserInteractionNodeConfig = {
      operationType: "ADD_MESSAGE",
      prompt: "Say something:",
      message: { role: "user", contentTemplate: "{{input}}" },
      timeout: 5000,
    };
    const node = { id: "ui-node-2", type: "USER_INTERACTION", config } as RuntimeNode;

    const result = await userInteractionHandler(mockExecution, node, defaultContext);

    expect(result.operationType).toBe("ADD_MESSAGE");
    expect(mockConversationManager.addMessage).toHaveBeenCalledWith({
      role: "user",
      content: "user input data",
    });
  });

  it("should throw ExecutionError for unknown operation type", async () => {
    const config = {
      operationType: "UNKNOWN" as any,
      prompt: "test",
      timeout: 5000,
    };
    const node = { id: "ui-node-3", type: "USER_INTERACTION", config } as RuntimeNode;

    await expect(userInteractionHandler(mockExecution, node, defaultContext)).rejects.toThrow(
      "Unknown operation type: UNKNOWN",
    );
  });
});
