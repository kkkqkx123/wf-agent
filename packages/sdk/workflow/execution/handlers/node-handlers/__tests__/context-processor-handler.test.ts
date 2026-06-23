import { describe, it, expect, vi, beforeEach } from "vitest";
import { contextProcessorHandler } from "../context-processor-handler.js";
import type { ContextProcessorNodeConfig, RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../../entities/workflow-execution-entity.js";
import type { ContextProcessorHandlerContext } from "../context-processor-handler.js";

// Mock VariableManager
class MockVariableManager {
  private variables: Record<string, unknown> = {};

  setVariable(name: string, value: unknown) {
    this.variables[name] = value;
  }

  getVariable(name: string): unknown {
    return this.variables[name];
  }

  getAllVariables(): Record<string, unknown> {
    return { ...this.variables };
  }
}

// Helper to create mock execution entity
function createMockExecutionEntity(
  variables: Record<string, unknown> = {}
): WorkflowExecutionEntity {
  const variableManager = new MockVariableManager();
  Object.entries(variables).forEach(([name, value]) => {
    variableManager.setVariable(name, value);
  });

  return {
    variableStateManager: variableManager as any,
    getWorkflowExecutionData: () => ({
      id: "exec-1",
      workflowId: "wf-1",
      variables: Object.entries(variables).map(([name, value]) => ({
        name,
        value,
        type: typeof value,
      })),
      nodeResults: [],
      errors: [],
      input: {},
      output: {},
    } as any),
    addNodeResult: vi.fn(),
    getNodeResults: () => [],
  } as unknown as WorkflowExecutionEntity;
}

const mockConversationManager = {
  executeMessageOperation: vi.fn(),
  getMessages: vi.fn(),
  clearMessages: vi.fn(),
  addMessages: vi.fn(),
};

const mockRegistry = {
  get: vi.fn(),
  has: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  delete: vi.fn(),
  listIds: vi.fn(),
};

const defaultContext: ContextProcessorHandlerContext = {
  conversationManager: mockConversationManager as any,
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockRegistry.get as any).mockReturnValue({
    id: "current",
    messages: [{ role: "user", content: "test" }],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
  (mockRegistry.has as any).mockReturnValue(true);
  mockConversationManager.getMessages.mockReturnValue([{ role: "user", content: "processed" }]);
  mockConversationManager.executeMessageOperation.mockImplementation(
    async (_config: unknown, _onAfterOperation?: (result: any) => Promise<void>) => {
      return {
        stats: { originalMessageCount: 1, visibleMessageCount: 1, invisibleMessageCount: 0 },
      } as any;
    },
  );
});

describe("contextProcessorHandler - Message Operations", () => {
  it("should execute message operation and return result", async () => {
    const executionEntity = createMockExecutionEntity();
    (executionEntity.getWorkflowExecutionData as any).mockReturnValue({
      id: "exec-1",
      workflowId: "wf-1",
      messageContextRegistry: mockRegistry,
      variables: [],
      nodeResults: [],
      errors: [],
      input: {},
      output: {},
    });

    const config: ContextProcessorNodeConfig = {
      operationConfig: {
        operation: "TRUNCATE",
        strategy: { type: "KEEP_LAST", count: 10 },
      } as any,
    };
    const node: RuntimeNode = { id: "cp-node-1", type: "CONTEXT_PROCESSOR", config } as any;

    const result = await contextProcessorHandler(executionEntity, node, defaultContext);

    expect(result.operation).toBe("TRUNCATE");
    expect(result.messageCount).toBeGreaterThan(0);
    expect(result.stats).toBeDefined();
  });

  it("should throw error when neither operationConfig nor variableOperation specified", async () => {
    const executionEntity = createMockExecutionEntity();
    const config: ContextProcessorNodeConfig = {
      version: 4,
    };
    const node: RuntimeNode = { id: "cp-node-2", type: "CONTEXT_PROCESSOR", config } as any;

    await expect(contextProcessorHandler(executionEntity, node, defaultContext)).rejects.toThrow(
      "Either operationConfig (message) or variableOperation must be specified"
    );
  });

  it("should handle custom source and target contexts for message operations", async () => {
    const executionEntity = createMockExecutionEntity();
    (executionEntity.getWorkflowExecutionData as any).mockReturnValue({
      id: "exec-1",
      workflowId: "wf-1",
      messageContextRegistry: mockRegistry,
      variables: [],
      nodeResults: [],
      errors: [],
      input: {},
      output: {},
    });

    (mockRegistry.has as any).mockReturnValue(false);
    (mockRegistry.get as any).mockImplementation((id: string) => {
      if (id === "source")
        return {
          id: "source",
          messages: [{ role: "user", content: "from source" }],
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };
      if (id === "target") return undefined;
      return undefined;
    });

    const config: ContextProcessorNodeConfig = {
      sourceContext: "source",
      targetContext: "target",
      operationConfig: { operation: "APPEND", messages: [] } as any,
    };
    const node: RuntimeNode = { id: "cp-node-3", type: "CONTEXT_PROCESSOR", config } as any;

    const result = await contextProcessorHandler(executionEntity, node, defaultContext);

    expect(result.operation).toBe("APPEND");
    expect(mockRegistry.register).toHaveBeenCalledWith(expect.objectContaining({ id: "target" }));
  });

  it("should target parent execution when configured", async () => {
    const executionEntity = createMockExecutionEntity();
    (executionEntity.getWorkflowExecutionData as any).mockReturnValue({
      id: "exec-1",
      workflowId: "wf-1",
      messageContextRegistry: mockRegistry,
      variables: [],
      nodeResults: [],
      errors: [],
      input: {},
      output: {},
    });

    const mockParentConversationManager = {
      executeMessageOperation: vi
        .fn()
        .mockImplementation(
          async (_config: unknown, _onAfterOperation?: (result: any) => Promise<void>) => {
            return {
              stats: { originalMessageCount: 0, visibleMessageCount: 0, invisibleMessageCount: 0 },
            };
          }
        ),
      getMessages: vi.fn().mockReturnValue([]),
      clearMessages: vi.fn(),
      addMessages: vi.fn(),
    };

    const contextWithParent: ContextProcessorHandlerContext = {
      conversationManager: mockConversationManager as any,
      executionEntity: {
        getParentContext: vi.fn().mockReturnValue({ parentType: "WORKFLOW", parentId: "parent-1" }),
        getConversationManager: vi.fn(),
      },
      executionRegistry: {
        get: vi.fn().mockReturnValue({
          getConversationManager: vi.fn().mockReturnValue(mockParentConversationManager),
        }),
      },
    };

    const config: ContextProcessorNodeConfig = {
      operationConfig: { operation: "CLEAR" },
      operationOptions: { target: "parent" },
    };
    const node: RuntimeNode = { id: "cp-node-4", type: "CONTEXT_PROCESSOR", config } as any;

    const result = await contextProcessorHandler(executionEntity, node, contextWithParent);

    expect(result.operation).toBe("CLEAR");
  });
});
