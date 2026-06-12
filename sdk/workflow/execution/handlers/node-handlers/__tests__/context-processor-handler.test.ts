import { describe, it, expect, vi, beforeEach } from "vitest";
import { contextProcessorHandler } from "../context-processor-handler.js";
import type { WorkflowExecution, ContextProcessorNodeConfig } from "@wf-agent/types";
import type { RuntimeNode } from "@wf-agent/types";
import type { ContextProcessorHandlerContext } from "../context-processor-handler.js";

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

const mockExecution = {
  id: "exec-1",
  workflowId: "wf-1",
  messageContextRegistry: mockRegistry,
} as unknown as WorkflowExecution;

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

describe("contextProcessorHandler", () => {
  it("should execute message operation and return result", async () => {
    const config: ContextProcessorNodeConfig = {
      operationConfig: {
        operation: "TRUNCATE",
        strategy: { type: "KEEP_LAST", count: 10 },
      } as any,
    };
    const node = { id: "cp-node-1", type: "CONTEXT_PROCESSOR", config } as RuntimeNode;

    const result = await contextProcessorHandler(mockExecution, node, defaultContext);

    expect(result.operation).toBe("TRUNCATE");
    expect(result.messageCount).toBeGreaterThan(0);
    expect(result.stats).toBeDefined();
  });

  it("should throw RuntimeValidationError when operationConfig missing", async () => {
    const config = {} as ContextProcessorNodeConfig;
    const node = { id: "cp-node-2", type: "CONTEXT_PROCESSOR", config } as RuntimeNode;

    await expect(contextProcessorHandler(mockExecution, node, defaultContext)).rejects.toThrow(
      "operationConfig is required",
    );
  });

  it("should handle custom source and target contexts", async () => {
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
    const node = { id: "cp-node-3", type: "CONTEXT_PROCESSOR", config } as RuntimeNode;

    const result = await contextProcessorHandler(mockExecution, node, defaultContext);

    expect(result.operation).toBe("APPEND");
    expect(mockRegistry.register).toHaveBeenCalledWith(expect.objectContaining({ id: "target" }));
  });

  it("should target parent execution when configured", async () => {
    const mockParentConversationManager = {
      executeMessageOperation: vi
        .fn()
        .mockImplementation(
          async (_config: unknown, _onAfterOperation?: (result: any) => Promise<void>) => {
            return {
              stats: { originalMessageCount: 0, visibleMessageCount: 0, invisibleMessageCount: 0 },
            };
          },
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
    const node = { id: "cp-node-4", type: "CONTEXT_PROCESSOR", config } as RuntimeNode;

    const result = await contextProcessorHandler(mockExecution, node, contextWithParent);

    expect(result.operation).toBe("CLEAR");
  });
});
