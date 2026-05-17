import { describe, it, expect, beforeEach } from 'vitest';
import { LLMContextFactory, LLMContextFactoryConfig } from '../llm-context-factory.js';
import { ExecutionError } from '@wf-agent/types';

// Mock types for testing
interface MockWorkflowExecutionRegistry {
  get: (id: string) => any;
  set: (id: string, value: any) => void;
  has: (id: string) => boolean;
  delete: (id: string) => boolean;
  clear: () => void;
  size: number;
  [Symbol.iterator]: () => IterableIterator<[string, any]>;
  entries: () => IterableIterator<[string, any]>;
  keys: () => IterableIterator<string>;
  values: () => IterableIterator<any>;
  forEach: (callbackfn: (value: any, key: string, map: Map<string, any>) => void, thisArg?: any) => void;
}
interface MockWorkflowRegistry {
  get: (id: string) => any;
  set: (id: string, value: any) => void;
  has: (id: string) => boolean;
  delete: (id: string) => boolean;
  clear: () => void;
  size: number;
  [Symbol.iterator]: () => IterableIterator<[string, any]>;
  entries: () => IterableIterator<[string, any]>;
  keys: () => IterableIterator<string>;
  values: () => IterableIterator<any>;
  forEach: (callbackfn: (value: any, key: string, map: Map<string, any>) => void, thisArg?: any) => void;
}
interface MockWorkflowGraphRegistry {
  workflowGraphs: Map<string, any>;
  register: (id: string, graph: any) => void;
  get: (id: string) => any | undefined;
  has: (id: string) => boolean;
  unregister: (id: string) => boolean;
  list: () => string[];
  clear: () => void;
}
interface MockEventRegistry {
  emitters: Map<string, any>;
  metricsCollector?: any;
  getEmitter: (name: string) => any;
  on: (event: string, handler: (...args: any[]) => void) => void;
  off: (event: string, handler: (...args: any[]) => void) => void;
  emit: (event: string, ...args: any[]) => void;
  once: (event: string, handler: (...args: any[]) => void) => void;
  removeAllListeners: (event?: string) => void;
  listenerCount: (event: string) => number;
  listeners: (event: string) => Array<(...args: any[]) => void>;
  rawListeners: (event: string) => Array<(...args: any[]) => void>;
}
interface MockToolRegistry {
  tools: Map<string, any>;
  register: (tool: any) => void;
  get: (name: string) => any | undefined;
  has: (name: string) => boolean;
  unregister: (name: string) => boolean;
  list: () => any[];
  clear: () => void;
}
interface MockLLMExecutor {
  execute: (request: any) => Promise<any>;
  getModelInfo: () => any;
  supportsStreaming: () => boolean;
}
interface MockToolCallExecutor {
  execute: (toolCall: any) => Promise<any>;
  validateToolCall: (toolCall: any) => boolean;
}
interface MockInterruptionDetector {
  isInterrupted: () => boolean;
  checkForInterruption: () => void;
  reset: () => void;
}
interface MockCheckpointState {
  cleanupWorkflowExecutionCheckpoints: (executionId: string) => Promise<void>;
  create: (executionId: string, nodeId: string, state: any) => Promise<string>;
  get: (checkpointId: string) => Promise<any | undefined>;
  list: (executionId: string) => Promise<any[]>;
  restore: (checkpointId: string) => Promise<any>;
  delete: (checkpointId: string) => Promise<boolean>;
  exists: (checkpointId: string) => Promise<boolean>;
  update: (checkpointId: string, state: any) => Promise<boolean>;
  getLatest: (executionId: string) => Promise<any | undefined>;
  getLatestByNode: (executionId: string, nodeId: string) => Promise<any | undefined>;
  count: (executionId: string) => Promise<number>;
  clear: (executionId: string) => Promise<void>;
}

// Create mock implementations
const createMockWorkflowExecutionRegistry = (): any => ({
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
});

const createMockWorkflowRegistry = (): any => ({
  workflows: new Map(),
  workflowRelationships: new Map(),
  activeWorkflows: new Set(),
  referenceRelations: new Map(),
  register: () => {},
  get: () => undefined,
  has: () => false,
  unregister: () => false,
  list: () => [],
  clear: () => {},
  size: () => 0,
  getAllWorkflowIds: () => [],
});

const createMockWorkflowGraphRegistry = (): any => ({
  workflowGraphs: new Map(),
  register: () => {},
  get: () => undefined,
  has: () => false,
  unregister: () => false,
  list: () => [],
  clear: () => {},
  size: () => 0,
  getAllWorkflowIds: () => [],
  registerBatch: () => {},
  unregisterBatch: () => {},
});

const createMockEventRegistry = (): any => ({
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
});

const createMockToolRegistry = (): any => ({
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
});

const createMockLLMExecutor = (): any => ({
  execute: () => Promise.resolve({}),
  getModelInfo: () => ({}),
  supportsStreaming: () => false,
});

const createMockToolCallExecutor = (): any => ({
  execute: () => Promise.resolve({}),
  validateToolCall: () => true,
});

const createMockInterruptionDetector = (): any => ({
  isInterrupted: () => false,
  checkForInterruption: () => {},
  reset: () => {},
});

const createMockCheckpointState = (): any => ({
  cleanupWorkflowExecutionCheckpoints: () => Promise.resolve(),
  create: () => Promise.resolve(''),
  get: () => Promise.resolve(undefined),
  list: () => Promise.resolve([]),
  restore: () => Promise.resolve({}),
  delete: () => Promise.resolve(false),
  exists: () => Promise.resolve(false),
  update: () => Promise.resolve(false),
  getLatest: () => Promise.resolve(undefined),
  getLatestByNode: () => Promise.resolve(undefined),
  count: () => Promise.resolve(0),
  clear: () => Promise.resolve(),
  extractStorageMetadata: () => ({}),
  buildCreatedEvent: () => ({}),
  buildDeletedEvent: () => ({}),
  buildFailedEvent: () => ({}),
});

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