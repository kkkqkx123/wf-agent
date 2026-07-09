import { describe, it, expect, vi, beforeEach } from "vitest";
import { agentLoopHandler } from "../agent-loop-handler.js";
import type { GlobalContext } from "../../../../../shared/global-context.js";
import type { AgentLoopNodeConfig } from "@wf-agent/types";
import type { RuntimeNode } from "@wf-agent/types";
import type { AgentLoopHandlerContext } from "../agent-loop-handler.js";

const mockGlobalContext = {
  container: {
    get: vi.fn(),
  },
} as unknown as GlobalContext;

const mockLLMExecutor = {};
const mockToolService = {};
const mockEventManager = { emit: vi.fn() };
const mockConversationManager = {
  addMessage: vi.fn(),
  getMessages: vi.fn().mockReturnValue([]),
  getAllMessages: vi.fn().mockReturnValue([]),
  getMessageCount: vi.fn().mockReturnValue(0),
};

const mockRegistry = {
  get: vi.fn(),
  has: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  getAll: vi.fn(),
};

const mockEntity = {
  getWorkflowExecutionData: vi.fn().mockReturnValue({
    id: "exec-1",
    variableScopes: {
      execution: {},
    },
    messageContextRegistry: mockRegistry,
  }),
  variableStateManager: {
    getVariable: vi.fn(),
    setVariable: vi.fn(),
  },
  getInput: vi.fn(),
};

const defaultContext: AgentLoopHandlerContext = {
  llmExecutor: mockLLMExecutor as any,
  toolService: mockToolService as any,
  conversationManager: mockConversationManager as any,
  eventManager: mockEventManager as any,
  workflowExecutionEntity: mockEntity as any,
};

const mockExecute = vi.fn().mockResolvedValue({
  success: true,
  content: "Agent response",
  iterations: 2,
  toolCallCount: 3,
});

vi.mock("../../../../../agent/index.js", () => ({
  AgentLoopCoordinator: vi.fn().mockImplementation(function () {
    return { execute: mockExecute };
  }),
  AgentLoopExecutor: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
  mockExecute.mockResolvedValue({
    success: true,
    content: "Agent response",
    iterations: 2,
    toolCallCount: 3,
  });
  mockRegistry.get.mockReturnValue({
    id: "current",
    messages: [{ role: "system", content: "You are a helpful assistant." }],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
});

describe("agentLoopHandler", () => {
  it("should execute agent loop and return result", async () => {
    const config: AgentLoopNodeConfig = {
      inlineConfig: {
        profileId: "gpt-4-agent",
        maxIterations: 5,
        availableTools: { tools: ["tool1", "tool2"] },
      },
    };
    const node = { id: "agent-loop-1", type: "AGENT_LOOP", config } as RuntimeNode;

    const result = await agentLoopHandler(mockGlobalContext, mockEntity, node, defaultContext);

    if ((result as any).status !== "COMPLETED") {
      console.error("Handler error:", (result as any).error);
    }
    expect((result as any).status).toBe("COMPLETED");
    expect((result as any).content).toBe("Agent response");
    expect((result as any).iterations).toBe(2);
    expect((result as any).toolCallCount).toBe(3);
  });

  it("should return FAILED when profileId is missing", async () => {
    const config: AgentLoopNodeConfig = {
      inlineConfig: {
        profileId: "",
      },
    };
    const node = { id: "agent-loop-2", type: "AGENT_LOOP", config } as RuntimeNode;

    const result = await agentLoopHandler(mockGlobalContext, mockEntity, node, defaultContext);

    expect(result.status).toBe("FAILED");
    expect(result.error).toBeDefined();
  });

  // TODO: Update this test to use the new variable state manager architecture
  it.skip("should add input prompt from variables when available", async () => {
    // This test needs to be updated to work with the new VariableManager architecture
    // mockEntity.variableStateManager.setVariable('input', 'User query here', 'execution');

    const config: AgentLoopNodeConfig = {
      inlineConfig: {
        profileId: "gpt-4-agent",
      },
    };
    const node = { id: "agent-loop-3", type: "AGENT_LOOP", config } as RuntimeNode;

    await agentLoopHandler(mockGlobalContext, mockEntity, node, defaultContext);

    expect(mockEventManager.emit).toHaveBeenCalled();
  });

  it("should handle agent loop execution failure", async () => {
    mockExecute.mockResolvedValue({
      success: false,
      error: new Error("Agent loop failed"),
    });

    const config: AgentLoopNodeConfig = {
      inlineConfig: {
        profileId: "gpt-4-agent",
      },
    };
    const node = { id: "agent-loop-4", type: "AGENT_LOOP", config } as RuntimeNode;

    const result = await agentLoopHandler(mockGlobalContext, mockEntity, node, defaultContext);

    expect(result.status).toBe("FAILED");
  });

  it("should handle unexpected errors gracefully", async () => {
    mockExecute.mockRejectedValue(new Error("Unexpected error"));

    const config: AgentLoopNodeConfig = {
      inlineConfig: {
        profileId: "gpt-4-agent",
      },
    };
    const node = { id: "agent-loop-5", type: "AGENT_LOOP", config } as RuntimeNode;

    const result = await agentLoopHandler(mockGlobalContext, mockEntity, node, defaultContext);

    expect(result.status).toBe("FAILED");
  });
});
