import { describe, it, expect, vi, beforeEach } from "vitest";
import { llmHandler } from "../llm-handler.js";
import type { WorkflowExecution, LLMNodeConfig } from "@wf-agent/types";
import type { LLMHandlerContext } from "../llm-handler.js";
import type { RuntimeNode } from "@wf-agent/types";

const mockLLMCoordinator = {
  executeLLM: vi.fn(),
};

const mockLLMWrapper = {
  getProfile: vi.fn(),
};

const mockEventManager = {
  emit: vi.fn(),
};

const mockConversationManager = {
  addMessage: vi.fn(),
  getMessages: vi.fn().mockReturnValue([]),
};

const mockRegistry = {
  get: vi.fn(),
  has: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  delete: vi.fn(),
  listIds: vi.fn(),
} as any;

const defaultContext: LLMHandlerContext = {
  llmCoordinator: mockLLMCoordinator as any,
  llmWrapper: mockLLMWrapper as any,
  eventManager: mockEventManager as any,
  conversationManager: mockConversationManager as any,
};

const mockExecution = {
  id: "exec-1",
  messageContextRegistry: mockRegistry,
} as unknown as WorkflowExecution;

beforeEach(() => {
  vi.clearAllMocks();
  mockRegistry.get.mockReturnValue({
    id: "current",
    messages: [{ role: "user", content: "hello" }],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
  mockLLMWrapper.getProfile.mockReturnValue({ provider: "OPENAI" });
  mockLLMCoordinator.executeLLM.mockResolvedValue({
    success: true,
    content: "LLM response",
  });
});

describe("llmHandler", () => {
  it("should collect messages, execute LLM, and return result", async () => {
    const config: LLMNodeConfig = {
      profileId: "gpt-4",
      parameters: { temperature: 0.7 },
    };
    const node = { id: "llm-node-1", type: "LLM", config } as RuntimeNode;

    const result = await llmHandler(mockExecution, node, defaultContext);

    expect(mockLLMCoordinator.executeLLM).toHaveBeenCalledWith(
      expect.objectContaining({
        executionId: "exec-1",
        nodeId: "llm-node-1",
        profileId: "gpt-4",
      }),
      mockConversationManager,
    );
    expect(result.status).toBe("COMPLETED");
    expect(result.content).toBe("LLM response");
  });

  it("should handle LLM execution failure", async () => {
    mockLLMCoordinator.executeLLM.mockResolvedValue({
      success: false,
      error: new Error("API error"),
    });

    const config: LLMNodeConfig = { profileId: "gpt-4" };
    const node = { id: "llm-node-2", type: "LLM", config } as RuntimeNode;

    const result = await llmHandler(mockExecution, node, defaultContext);

    expect(result.status).toBe("FAILED");
    expect(result.error).toBeDefined();
  });

  it("should handle catch error gracefully", async () => {
    mockLLMCoordinator.executeLLM.mockRejectedValue(new Error("Unexpected error"));

    const config: LLMNodeConfig = { profileId: "gpt-4" };
    const node = { id: "llm-node-3", type: "LLM", config } as RuntimeNode;

    const result = await llmHandler(mockExecution, node, defaultContext);

    expect(result.status).toBe("FAILED");
    expect(result.error).toBeDefined();
  });

  it("should throw RuntimeValidationError when context not found", async () => {
    mockRegistry.get.mockReturnValue(undefined);

    const config: LLMNodeConfig = {
      profileId: "gpt-4",
      contextId: "missing-ctx",
    };
    const node = { id: "llm-node-4", type: "LLM", config } as RuntimeNode;

    const result = await llmHandler(mockExecution, node, defaultContext);

    expect(result.status).toBe("FAILED");
  });

  it("should write response to outputContext when configured", async () => {
    mockRegistry.get.mockImplementation((id: string) => {
      if (id === "current")
        return {
          id: "current",
          messages: [{ role: "user", content: "hi" }],
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };
      if (id === "output-ctx")
        return { id: "output-ctx", messages: [], createdAt: Date.now(), updatedAt: Date.now() };
      return undefined;
    });

    const config: LLMNodeConfig = {
      profileId: "gpt-4",
      outputContext: "output-ctx",
    };
    const node = { id: "llm-node-5", type: "LLM", config } as RuntimeNode;

    await llmHandler(mockExecution, node, defaultContext);

    expect(mockRegistry.update).toHaveBeenCalledWith(
      "output-ctx",
      expect.arrayContaining([
        expect.objectContaining({ role: "assistant", content: "LLM response" }),
      ]),
    );
  });
});
