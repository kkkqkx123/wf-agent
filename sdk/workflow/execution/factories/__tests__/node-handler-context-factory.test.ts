import { describe, it, expect, beforeEach } from "vitest";
import {
  NodeHandlerContextFactory,
  type NodeHandlerContextFactoryConfig,
} from "../node-handler-context-factory.js";
import { ExecutionError } from "@wf-agent/types";
import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { UserInteractionHandler } from "@wf-agent/types";
import type { ConversationSession } from "../../../../core/messaging/conversation-session.js";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import type { ToolRegistry } from "../../../../core/registry/tool-registry.js";
import type { WorkflowExecutionRegistry } from "../../../stores/workflow-execution-registry.js";
import type { WorkflowExecutionBuilder } from "../workflow-execution-builder.js";
import type { WorkflowExecutor } from "../../executors/workflow-executor.js";
import type { LLMWrapper } from "../../../../core/llm/wrapper.js";
import { LLMExecutionCoordinator } from "../../coordinators/llm-execution-coordinator.js";

// Create mock implementations
const createMockRuntimeNode = (type: string): RuntimeNode =>
  ({
    id: "node123",
    type: type,
    name: "Test Node",
    config: {},
    workflowId: "test-workflow",
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
  }) as any;

const createMockWorkflowExecutionEntity = (): WorkflowExecutionEntity =>
  ({
    id: "exec123",
    workflowId: "workflow456",
    getWorkflowId: () => "workflow456",
    getWorkflowExecutionData: () => ({}) as any,
    variableStateManager: {} as any,
    messageHistoryManager: {} as any,
    setParentContext: () => {},
    registerChild: () => {},
    getHierarchyMetadata: () => undefined,
    getRootExecutionId: () => "",
    getRootExecutionType: () => "",
  }) as any;

const createMockEventRegistry = (): EventRegistry =>
  ({
    emitters: new Map(),
    metricsCollector: undefined,
    getEmitter: () => undefined,
    on: () => {},
    off: () => {},
    emit: () => {},
    once: () => {},
    removeAllListeners: () => {},
    listenerCount: () => 0,
    listeners: () => [],
    rawListeners: () => [],
    waitFor: () => Promise.resolve(),
    cleanupExecutionListeners: () => {},
    getExecutionListenerStats: () => ({}),
    getMetricsCollector: () => undefined,
  }) as any;

const createMockLLMExecutionCoordinator = (): LLMExecutionCoordinator =>
  ({
    executeLLMCall: () => Promise.resolve({}),
    handleToolCalls: () => Promise.resolve({}),
    checkInterruption: () => false,
  }) as any;

const createMockLLMWrapper = (): LLMWrapper =>
  ({
    getProfile: () => undefined,
    getAllProfiles: () => [],
    hasProfile: () => false,
  }) as any;

const createMockConversationSession = (): ConversationSession =>
  ({
    addMessage: () => {},
    getMessages: () => [],
    getContext: () => ({ workflowId: "", executionId: "" }),
    setContext: () => {},
  }) as any;

const createMockUserInteractionHandler = (): UserInteractionHandler =>
  ({
    handleInteraction: () => Promise.resolve({}),
  }) as any;

const createMockToolRegistry = (): ToolRegistry =>
  ({
    tools: new Map(),
    executors: new Map(),
    staticValidator: { validate: () => true },
    runtimeValidator: { validate: () => true },
    builtinExecutor: { execute: () => Promise.resolve({}) },
    register: () => {},
    get: () => undefined,
    has: () => false,
    unregister: () => false,
    list: () => [],
    clear: () => {},
    size: () => 0,
    getAllTools: () => [],
    getToolNames: () => [],
    executeTool: () => Promise.resolve({}),
    validateTool: () => true,
    getToolExecutor: () => undefined,
    setToolExecutor: () => {},
    removeToolExecutor: () => {},
    hasToolExecutor: () => false,
    getToolExecutors: () => [],
    clearToolExecutors: () => {},
    getToolMetadata: () => undefined,
    setToolMetadata: () => {},
    removeToolMetadata: () => {},
    hasToolMetadata: () => false,
    getToolMetadatas: () => [],
    clearToolMetadatas: () => {},
  }) as any;

const createMockWorkflowExecutionRegistry = (): WorkflowExecutionRegistry =>
  ({
    workflowExecutionEntities: new Map(),
    register: () => {},
    get: () => null,
    delete: () => {},
    getAll: () => [],
    getAllIds: () => [],
    size: () => 0,
    clear: () => {},
    has: () => false,
    isWorkflowActive: () => false,
    getByStatus: () => [],
    getActive: () => [],
    getExecutionState: () => undefined,
    setExecutionState: () => {},
    getExecutionStates: () => [],
    clearExecutionStates: () => {},
    getExecutionMetadata: () => undefined,
    setExecutionMetadata: () => {},
    removeExecutionMetadata: () => {},
    hasExecutionMetadata: () => false,
    getExecutionMetadatas: () => [],
    clearExecutionMetadatas: () => {},
  }) as any;

const createMockWorkflowExecutionBuilder = (): WorkflowExecutionBuilder =>
  ({
    build: () =>
      Promise.resolve({
        workflowExecutionEntity: {} as any,
        stateCoordinator: {} as any,
        conversationManager: {} as any,
      }),
    buildFromTemplate: () =>
      Promise.resolve({
        workflowExecutionEntity: {} as any,
        stateCoordinator: {} as any,
        conversationManager: {} as any,
      }),
    createCopy: () =>
      Promise.resolve({
        workflowExecutionEntity: {} as any,
        stateCoordinator: {} as any,
        conversationManager: {} as any,
      }),
    createChildExecution: () =>
      Promise.resolve({
        workflowExecutionEntity: {} as any,
        stateCoordinator: {} as any,
        conversationManager: {} as any,
      }),
    clearCache: () => {},
    invalidateWorkflow: () => {},
  }) as any;

const createMockWorkflowExecutor = (): WorkflowExecutor =>
  ({
    execute: () => Promise.resolve({}),
  }) as any;

const createMockAgentLoopExecutorFactory = () => ({
  create: () => ({
    execute: () => Promise.resolve({}),
  }),
});

describe("NodeHandlerContextFactory", () => {
  let factory: NodeHandlerContextFactory;
  let config: NodeHandlerContextFactoryConfig;
  let executionEntity: WorkflowExecutionEntity;

  beforeEach(() => {
    config = {
      eventManager: createMockEventRegistry(),
      llmCoordinator: createMockLLMExecutionCoordinator(),
      llmWrapper: createMockLLMWrapper(),
      conversationManager: createMockConversationSession(),
      userInteractionHandler: createMockUserInteractionHandler(),
      toolService: createMockToolRegistry(),
      agentLoopExecutorFactory: createMockAgentLoopExecutorFactory(),
      workflowExecutionRegistry: createMockWorkflowExecutionRegistry(),
      executionBuilder: createMockWorkflowExecutionBuilder(),
      workflowExecutor: createMockWorkflowExecutor(),
    };
    factory = new NodeHandlerContextFactory(config);
    executionEntity = createMockWorkflowExecutionEntity();
  });

  describe("createHandlerContext", () => {
    it("should return empty context for unknown node types", () => {
      const node = createMockRuntimeNode("UNKNOWN_TYPE");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toEqual({});
    });

    it("should create USER_INTERACTION context with required dependencies", () => {
      const node = createMockRuntimeNode("USER_INTERACTION");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toHaveProperty("userInteractionHandler");
      expect(result).toHaveProperty("conversationManager");
      expect((result as any).userInteractionHandler).toBe(config.userInteractionHandler);
      expect((result as any).conversationManager).toBe(config.conversationManager);
    });

    it("should throw ExecutionError when userInteractionHandler is missing for USER_INTERACTION node", () => {
      const configWithoutHandler = {
        ...config,
        userInteractionHandler: undefined,
      };
      const factoryWithoutHandler = new NodeHandlerContextFactory(configWithoutHandler);

      const node = createMockRuntimeNode("USER_INTERACTION");

      expect(() => factoryWithoutHandler.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutHandler.createHandlerContext(node, executionEntity)).toThrow(
        "UserInteractionHandler is required for USER_INTERACTION node",
      );
    });

    it("should create CONTEXT_PROCESSOR context with required dependencies", () => {
      const node = createMockRuntimeNode("CONTEXT_PROCESSOR");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toHaveProperty("conversationManager");
      expect(result).toHaveProperty("executionEntity");
      expect(result).toHaveProperty("workflowExecutionRegistry");
      expect((result as any).conversationManager).toBe(config.conversationManager);
      expect((result as any).executionEntity).toBe(executionEntity);
      expect((result as any).workflowExecutionRegistry).toBe(config.workflowExecutionRegistry);
    });

    it("should create LLM context with required dependencies", () => {
      const node = createMockRuntimeNode("LLM");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toHaveProperty("llmCoordinator");
      expect(result).toHaveProperty("llmWrapper");
      expect(result).toHaveProperty("eventManager");
      expect(result).toHaveProperty("conversationManager");
      expect((result as any).llmCoordinator).toBe(config.llmCoordinator);
      expect((result as any).llmWrapper).toBe(config.llmWrapper);
      expect((result as any).eventManager).toBe(config.eventManager);
      expect((result as any).conversationManager).toBe(config.conversationManager);
    });

    it("should throw ExecutionError when llmWrapper is missing for LLM node", () => {
      const configWithoutWrapper = {
        ...config,
        llmWrapper: undefined,
      } as any;
      const factoryWithoutWrapper = new NodeHandlerContextFactory(configWithoutWrapper);

      const node = createMockRuntimeNode("LLM");

      expect(() => factoryWithoutWrapper.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutWrapper.createHandlerContext(node, executionEntity)).toThrow(
        "LLMWrapper is required for LLM node",
      );
    });

    it("should create AGENT_LOOP context with required dependencies", () => {
      const node = createMockRuntimeNode("AGENT_LOOP");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toHaveProperty("agentLoopExecutor");
      expect(result).toHaveProperty("llmCoordinator");
      expect(result).toHaveProperty("conversationManager");
      expect(result).toHaveProperty("eventManager");
      expect((result as any).llmCoordinator).toBe(config.llmCoordinator);
      expect((result as any).conversationManager).toBe(config.conversationManager);
      expect((result as any).eventManager).toBe(config.eventManager);
    });

    it("should throw ExecutionError when agentLoopExecutorFactory is missing for AGENT_LOOP node", () => {
      const configWithoutFactory = {
        ...config,
        agentLoopExecutorFactory: undefined,
      };
      const factoryWithoutFactory = new NodeHandlerContextFactory(configWithoutFactory);

      const node = createMockRuntimeNode("AGENT_LOOP");

      expect(() => factoryWithoutFactory.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutFactory.createHandlerContext(node, executionEntity)).toThrow(
        "AgentLoopExecutorFactory is required for AGENT_LOOP node",
      );
    });

    it("should create FORK context with required dependencies", () => {
      const node = createMockRuntimeNode("FORK");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toHaveProperty("executionBuilder");
      expect(result).toHaveProperty("workflowExecutor");
      expect((result as any).executionBuilder).toBe(config.executionBuilder);
      expect((result as any).workflowExecutor).toBe(config.workflowExecutor);
    });

    it("should throw ExecutionError when executionBuilder is missing for FORK node", () => {
      const configWithoutBuilder = {
        ...config,
        executionBuilder: undefined,
      };
      const factoryWithoutBuilder = new NodeHandlerContextFactory(configWithoutBuilder);

      const node = createMockRuntimeNode("FORK");

      expect(() => factoryWithoutBuilder.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutBuilder.createHandlerContext(node, executionEntity)).toThrow(
        "WorkflowExecutionBuilder is required for FORK node",
      );
    });

    it("should throw ExecutionError when workflowExecutor is missing for FORK node", () => {
      const configWithoutExecutor = {
        ...config,
        workflowExecutor: undefined,
      };
      const factoryWithoutExecutor = new NodeHandlerContextFactory(configWithoutExecutor);

      const node = createMockRuntimeNode("FORK");

      expect(() => factoryWithoutExecutor.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutExecutor.createHandlerContext(node, executionEntity)).toThrow(
        "WorkflowExecutor is required for FORK node",
      );
    });

    it("should create SUBGRAPH context with required dependencies", () => {
      const node = createMockRuntimeNode("SUBGRAPH");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toHaveProperty("executionBuilder");
      expect(result).toHaveProperty("workflowExecutor");
      expect((result as any).executionBuilder).toBe(config.executionBuilder);
      expect((result as any).workflowExecutor).toBe(config.workflowExecutor);
    });

    it("should throw ExecutionError when executionBuilder is missing for SUBGRAPH node", () => {
      const configWithoutBuilder = {
        ...config,
        executionBuilder: undefined,
      };
      const factoryWithoutBuilder = new NodeHandlerContextFactory(configWithoutBuilder);

      const node = createMockRuntimeNode("SUBGRAPH");

      expect(() => factoryWithoutBuilder.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutBuilder.createHandlerContext(node, executionEntity)).toThrow(
        "WorkflowExecutionBuilder is required for SUBGRAPH node",
      );
    });

    it("should throw ExecutionError when workflowExecutor is missing for SUBGRAPH node", () => {
      const configWithoutExecutor = {
        ...config,
        workflowExecutor: undefined,
      };
      const factoryWithoutExecutor = new NodeHandlerContextFactory(configWithoutExecutor);

      const node = createMockRuntimeNode("SUBGRAPH");

      expect(() => factoryWithoutExecutor.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutExecutor.createHandlerContext(node, executionEntity)).toThrow(
        "WorkflowExecutor is required for SUBGRAPH node",
      );
    });

    it("should create START_FROM_TRIGGER context with conversationManager", () => {
      const node = createMockRuntimeNode("START_FROM_TRIGGER");
      const result = factory.createHandlerContext(node, executionEntity);

      expect(result).toHaveProperty("conversationManager");
      expect((result as any).conversationManager).toBe(config.conversationManager);
    });

    it("should create START_FROM_TRIGGER context even without conversationManager (optional)", () => {
      const configWithoutConversation = {
        ...config,
        conversationManager: undefined,
      } as any;
      const factoryWithoutConversation = new NodeHandlerContextFactory(configWithoutConversation);

      const node = createMockRuntimeNode("START_FROM_TRIGGER");
      const result = factoryWithoutConversation.createHandlerContext(node, executionEntity);

      // Should not throw, conversationManager is optional for START_FROM_TRIGGER
      expect(result).toEqual({});
    });

    it("should throw ExecutionError when conversationManager is missing for CONTEXT_PROCESSOR node", () => {
      const configWithoutConversation = {
        ...config,
        conversationManager: undefined,
      } as any;
      const factoryWithoutConversation = new NodeHandlerContextFactory(configWithoutConversation);

      const node = createMockRuntimeNode("CONTEXT_PROCESSOR");

      expect(() => factoryWithoutConversation.createHandlerContext(node, executionEntity)).toThrow(
        ExecutionError,
      );
      expect(() => factoryWithoutConversation.createHandlerContext(node, executionEntity)).toThrow(
        "ConversationManager is required for CONTEXT_PROCESSOR node",
      );
    });
  });
});
