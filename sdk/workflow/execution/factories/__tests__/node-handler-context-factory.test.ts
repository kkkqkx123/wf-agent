import { describe, it, expect, beforeEach } from 'vitest';
import { NodeHandlerContextFactory, NodeHandlerContextFactoryConfig } from '../node-handler-context-factory';
import { ExecutionError } from '@wf-agent/types';

// Mock types for testing
interface MockRuntimeNode {}
interface MockWorkflowExecutionEntity {}
interface MockEventRegistry {}
interface MockLLMExecutionCoordinator {}
interface MockConversationSession {}
interface MockUserInteractionHandler {}
interface MockHumanRelayHandler {}

// Create mock implementations
const createMockRuntimeNode = (type: string): MockRuntimeNode => ({
  id: 'node123',
  type,
} as MockRuntimeNode);

const createMockWorkflowExecutionEntity = (): MockWorkflowExecutionEntity => ({
  getWorkflowId: () => 'workflow123',
} as MockWorkflowExecutionEntity);

const createMockEventRegistry = (): MockEventRegistry => ({} as MockEventRegistry);
const createMockLLMExecutionCoordinator = (): MockLLMExecutionCoordinator => ({} as MockLLMExecutionCoordinator);
const createMockConversationSession = (): MockConversationSession => ({} as MockConversationSession);
const createMockUserInteractionHandler = (): MockUserInteractionHandler => ({} as MockUserInteractionHandler);
const createMockHumanRelayHandler = (): MockHumanRelayHandler => ({} as MockHumanRelayHandler);

describe('NodeHandlerContextFactory', () => {
  let factory: NodeHandlerContextFactory;
  let config: NodeHandlerContextFactoryConfig;

  beforeEach(() => {
    config = {
      eventManager: createMockEventRegistry(),
      llmCoordinator: createMockLLMExecutionCoordinator(),
      conversationManager: createMockConversationSession(),
      userInteractionHandler: createMockUserInteractionHandler(),
      humanRelayHandler: createMockHumanRelayHandler(),
      toolContextStore: {},
      toolService: {},
      agentLoopExecutorFactory: {
        create: () => ({})
      },
      executionRegistry: {},
      workflowExecutionRegistry: {},
    };
    factory = new NodeHandlerContextFactory(config);
  });

  describe('createHandlerContext', () => {
    it('should create user interaction context when node type is USER_INTERACTION', () => {
      const node = createMockRuntimeNode('USER_INTERACTION');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      const result = factory.createHandlerContext(node, executionEntity);
      
      expect(result.userInteractionHandler).toBe(config.userInteractionHandler);
      expect(result.conversationManager).toBe(config.conversationManager);
    });

    it('should throw ExecutionError when userInteractionHandler is missing and node type is USER_INTERACTION', () => {
      // Remove userInteractionHandler from config
      const configWithoutHandler = {
        ...config,
        userInteractionHandler: undefined,
      };
      const factoryWithoutHandler = new NodeHandlerContextFactory(configWithoutHandler);
      
      const node = createMockRuntimeNode('USER_INTERACTION');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      expect(() => 
        factoryWithoutHandler.createHandlerContext(node, executionEntity)
      ).toThrow(ExecutionError);
      expect(() => 
        factoryWithoutHandler.createHandlerContext(node, executionEntity)
      ).toThrow('UserInteractionHandler is not provided');
    });

    it('should create context processor context when node type is CONTEXT_PROCESSOR', () => {
      const node = createMockRuntimeNode('CONTEXT_PROCESSOR');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      const result = factory.createHandlerContext(node, executionEntity);
      
      expect(result.conversationManager).toBe(config.conversationManager);
      expect(result.executionEntity).toBe(executionEntity);
      expect(result.workflowExecutionRegistry).toBe(config.executionRegistry);
    });

    it('should create LLM context when node type is LLM', () => {
      const node = createMockRuntimeNode('LLM');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      const result = factory.createHandlerContext(node, executionEntity);
      
      expect(result.llmCoordinator).toBe(config.llmCoordinator);
      expect(result.eventManager).toBe(config.eventManager);
      expect(result.conversationManager).toBe(config.conversationManager);
      expect(result.humanRelayHandler).toBe(config.humanRelayHandler);
    });

    it('should create add tool context when node type is ADD_TOOL', () => {
      const node = createMockRuntimeNode('ADD_TOOL');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      const result = factory.createHandlerContext(node, executionEntity);
      
      expect(result.toolContextStore).toBe(config.toolContextStore);
      expect(result.toolService).toBe(config.toolService);
      expect(result.eventManager).toBe(config.eventManager);
      expect(result.executionEntity).toBe(executionEntity);
    });

    it('should throw ExecutionError when toolContextStore or toolService is missing and node type is ADD_TOOL', () => {
      // Remove toolContextStore from config
      const configWithoutToolContext = {
        ...config,
        toolContextStore: undefined,
      };
      const factoryWithoutToolContext = new NodeHandlerContextFactory(configWithoutToolContext);
      
      const node = createMockRuntimeNode('ADD_TOOL');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      expect(() => 
        factoryWithoutToolContext.createHandlerContext(node, executionEntity)
      ).toThrow(ExecutionError);
      expect(() => 
        factoryWithoutToolContext.createHandlerContext(node, executionEntity)
      ).toThrow('ToolContextStore or ToolRegistry is not provided');
    });

    it('should create agent loop context when node type is AGENT_LOOP', () => {
      const node = createMockRuntimeNode('AGENT_LOOP');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      const result = factory.createHandlerContext(node, executionEntity);
      
      expect(result.agentLoopExecutor).toBeDefined();
      expect(result.llmCoordinator).toBe(config.llmCoordinator);
      expect(result.conversationManager).toBe(config.conversationManager);
      expect(result.eventManager).toBe(config.eventManager);
    });

    it('should throw ExecutionError when agentLoopExecutorFactory is missing and node type is AGENT_LOOP', () => {
      // Remove agentLoopExecutorFactory from config
      const configWithoutExecutorFactory = {
        ...config,
        agentLoopExecutorFactory: undefined,
      };
      const factoryWithoutExecutorFactory = new NodeHandlerContextFactory(configWithoutExecutorFactory);
      
      const node = createMockRuntimeNode('AGENT_LOOP');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      expect(() => 
        factoryWithoutExecutorFactory.createHandlerContext(node, executionEntity)
      ).toThrow(ExecutionError);
      expect(() => 
        factoryWithoutExecutorFactory.createHandlerContext(node, executionEntity)
      ).toThrow('AgentLoopExecutorFactory is not provided');
    });

    it('should return empty context for unknown node types', () => {
      const node = createMockRuntimeNode('UNKNOWN_TYPE');
      const executionEntity = createMockWorkflowExecutionEntity();
      
      const result = factory.createHandlerContext(node, executionEntity);
      
      expect(result).toEqual({});
    });
  });
});