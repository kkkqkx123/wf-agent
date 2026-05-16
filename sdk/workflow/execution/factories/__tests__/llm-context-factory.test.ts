import { describe, it, expect, beforeEach } from 'vitest';
import { LLMContextFactory, LLMContextFactoryConfig } from '../llm-context-factory';
import { ExecutionError } from '@wf-agent/types';

// Mock types for testing
interface MockWorkflowExecutionRegistry {}
interface MockWorkflowRegistry {}
interface MockWorkflowGraphRegistry {}
interface MockEventRegistry {}
interface MockToolRegistry {}
interface MockLLMExecutor {}
interface MockToolCallExecutor {}
interface MockInterruptionDetector {}
interface MockCheckpointState {}

// Create mock implementations
const createMockWorkflowExecutionRegistry = (): MockWorkflowExecutionRegistry => ({} as MockWorkflowExecutionRegistry);
const createMockWorkflowRegistry = (): MockWorkflowRegistry => ({} as MockWorkflowRegistry);
const createMockWorkflowGraphRegistry = (): MockWorkflowGraphRegistry => ({} as MockWorkflowGraphRegistry);
const createMockEventRegistry = (): MockEventRegistry => ({} as MockEventRegistry);
const createMockToolRegistry = (): MockToolRegistry => ({} as MockToolRegistry);
const createMockLLMExecutor = (): MockLLMExecutor => ({} as MockLLMExecutor);
const createMockToolCallExecutor = (): MockToolCallExecutor => ({} as MockToolCallExecutor);
const createMockInterruptionDetector = (): MockInterruptionDetector => ({} as MockInterruptionDetector);
const createMockCheckpointState = (): MockCheckpointState => ({} as MockCheckpointState);

describe('LLMContextFactory', () => {
  let factory: LLMContextFactory;
  let config: LLMContextFactoryConfig;

  beforeEach(() => {
    config = {
      llmExecutor: createMockLLMExecutor(),
      toolService: createMockToolRegistry(),
      eventManager: createMockEventRegistry(),
      toolCallExecutor: createMockToolCallExecutor(),
      executionRegistry: createMockWorkflowExecutionRegistry(),
      interruptionDetector: createMockInterruptionDetector(),
      checkpointStateManager: createMockCheckpointState(),
      workflowRegistry: createMockWorkflowRegistry(),
      graphRegistry: createMockWorkflowGraphRegistry(),
      toolContextStore: {},
      toolVisibilityCoordinator: {},
    };
    factory = new LLMContextFactory(config);
  });

  describe('createToolApprovalContext', () => {
    it('should return tool approval context with required dependencies', () => {
      const result = factory.createToolApprovalContext('exec123', 'node456');
      
      expect(result.executionRegistry).toBe(config.executionRegistry);
      expect(result.workflowExecutionRegistry).toBe(config.executionRegistry);
      expect(result.checkpointStateManager).toBe(config.checkpointStateManager);
      expect(result.workflowRegistry).toBe(config.workflowRegistry);
      expect(result.graphRegistry).toBe(config.graphRegistry);
    });

    it('should throw ExecutionError when executionRegistry is missing', () => {
      // Remove executionRegistry from config
      const configWithoutExecutionRegistry = {
        ...config,
        executionRegistry: undefined,
      };
      const factoryWithoutExecutionRegistry = new LLMContextFactory(configWithoutExecutionRegistry);
      
      expect(() => 
        factoryWithoutExecutionRegistry.createToolApprovalContext('exec123', 'node456')
      ).toThrow(ExecutionError);
      expect(() => 
        factoryWithoutExecutionRegistry.createToolApprovalContext('exec123', 'node456')
      ).toThrow('WorkflowExecutionRegistry is required for tool approval context');
    });

    it('should throw ExecutionError when checkpointStateManager is missing', () => {
      // Remove checkpointStateManager from config
      const configWithoutCheckpoint = {
        ...config,
        checkpointStateManager: undefined,
      };
      const factoryWithoutCheckpoint = new LLMContextFactory(configWithoutCheckpoint);
      
      expect(() => 
        factoryWithoutCheckpoint.createToolApprovalContext('exec123', 'node456')
      ).toThrow(ExecutionError);
      expect(() => 
        factoryWithoutCheckpoint.createToolApprovalContext('exec123', 'node456')
      ).toThrow('CheckpointState is required for tool approval context');
    });
  });

  describe('createInterruptionContext', () => {
    it('should return interruption context with available dependencies', () => {
      const result = factory.createInterruptionContext();
      
      expect(result.workflowExecutionRegistry).toBe(config.executionRegistry);
      expect(result.interruptionDetector).toBe(config.interruptionDetector);
    });

    it('should return interruption context with undefined interruptionDetector when not provided', () => {
      const configWithoutInterruption = {
        ...config,
        interruptionDetector: undefined,
      };
      const factoryWithoutInterruption = new LLMContextFactory(configWithoutInterruption);
      
      const result = factoryWithoutInterruption.createInterruptionContext();
      expect(result.interruptionDetector).toBeUndefined();
    });
  });

  describe('createToolExecutionContext', () => {
    it('should return tool execution context with all required dependencies', () => {
      const result = factory.createToolExecutionContext();
      
      expect(result.toolService).toBe(config.toolService);
      expect(result.toolCallExecutor).toBe(config.toolCallExecutor);
      expect(result.eventManager).toBe(config.eventManager);
    });
  });

  describe('createLLMCallContext', () => {
    it('should return LLM call context with all required dependencies', () => {
      const result = factory.createLLMCallContext();
      
      expect(result.llmExecutor).toBe(config.llmExecutor);
      expect(result.eventManager).toBe(config.eventManager);
      expect(result.toolService).toBe(config.toolService);
    });
  });

  describe('createToolVisibilityContext', () => {
    it('should return tool visibility context with all dependencies', () => {
      const result = factory.createToolVisibilityContext();
      
      expect(result.toolContextStore).toBe(config.toolContextStore);
      expect(result.toolVisibilityCoordinator).toBe(config.toolVisibilityCoordinator);
      expect(result.toolService).toBe(config.toolService);
    });
  });

  describe('hasToolApprovalSupport', () => {
    it('should return true when both executionRegistry and checkpointStateManager are present', () => {
      expect(factory.hasToolApprovalSupport()).toBe(true);
    });

    it('should return false when executionRegistry is missing', () => {
      const configWithoutExecution = {
        ...config,
        executionRegistry: undefined,
      };
      const factoryWithoutExecution = new LLMContextFactory(configWithoutExecution);
      
      expect(factoryWithoutExecution.hasToolApprovalSupport()).toBe(false);
    });

    it('should return false when checkpointStateManager is missing', () => {
      const configWithoutCheckpoint = {
        ...config,
        checkpointStateManager: undefined,
      };
      const factoryWithoutCheckpoint = new LLMContextFactory(configWithoutCheckpoint);
      
      expect(factoryWithoutCheckpoint.hasToolApprovalSupport()).toBe(false);
    });
  });

  describe('hasInterruptionSupport', () => {
    it('should return true when interruptionDetector is present', () => {
      expect(factory.hasInterruptionSupport()).toBe(true);
    });

    it('should return true when executionRegistry is present even if interruptionDetector is missing', () => {
      const configWithoutInterruption = {
        ...config,
        interruptionDetector: undefined,
      };
      const factoryWithoutInterruption = new LLMContextFactory(configWithoutInterruption);
      
      expect(factoryWithoutInterruption.hasInterruptionSupport()).toBe(true);
    });

    it('should return false when both interruptionDetector and executionRegistry are missing', () => {
      const configWithoutBoth = {
        ...config,
        interruptionDetector: undefined,
        executionRegistry: undefined,
      };
      const factoryWithoutBoth = new LLMContextFactory(configWithoutBoth);
      
      expect(factoryWithoutBoth.hasInterruptionSupport()).toBe(false);
    });
  });

  describe('getter methods', () => {
    it('should return workflow execution registry', () => {
      expect(factory.getWorkflowExecutionRegistry()).toBe(config.executionRegistry);
    });

    it('should return event manager', () => {
      expect(factory.getEventManager()).toBe(config.eventManager);
    });

    it('should return tool service', () => {
      expect(factory.getToolService()).toBe(config.toolService);
    });

    it('should return LLM executor', () => {
      expect(factory.getLLMExecutor()).toBe(config.llmExecutor);
    });

    it('should return tool call executor', () => {
      expect(factory.getToolCallExecutor()).toBe(config.toolCallExecutor);
    });

    it('should return checkpoint state manager', () => {
      expect(factory.getCheckpointStateManager()).toBe(config.checkpointStateManager);
    });

    it('should return workflow registry', () => {
      expect(factory.getWorkflowRegistry()).toBe(config.workflowRegistry);
    });

    it('should return graph registry', () => {
      expect(factory.getGraphRegistry()).toBe(config.graphRegistry);
    });

    it('should return tool context store', () => {
      expect(factory.getToolContextStore()).toBe(config.toolContextStore);
    });

    it('should return tool visibility coordinator', () => {
      expect(factory.getToolVisibilityCoordinator()).toBe(config.toolVisibilityCoordinator);
    });
  });
});