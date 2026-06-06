import { describe, it, expect, beforeEach } from 'vitest';
import { TriggerHandlerContextFactory, TriggerHandlerContextFactoryConfig, LifecycleTriggerContext, SkipNodeTriggerContext, SetVariableTriggerContext, ExecuteSubgraphTriggerContext } from '../trigger-handler-context-factory.js';
import { DependencyInjectionError } from '@wf-agent/types';

// Create mock implementations
const createMockTrigger = (actionType: string): any => ({
  id: 'trigger123',
  action: { type: actionType },
  executionId: 'exec456'
});

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

const createMockTriggerState = (): any => ({
  states: new Map(),
  executionId: '',
  workflowId: '',
  setWorkflowId: () => {},
  getWorkflowId: () => '',
  setExecutionId: () => {},
  getExecutionId: () => '',
  getState: () => undefined,
  setState: () => {},
  clearState: () => {},
  hasState: () => false,
  getAllStates: () => [],
  clearAllStates: () => {},
});

const createMockGlobalContext = (): any => ({
  container: {
    get: () => {},
    set: () => {},
    has: () => false,
  },
  workflowRegistry: undefined,
  toolRegistry: undefined,
  scriptRegistry: undefined,
  eventManager: undefined,
  llmExecutor: undefined,
  toolCallExecutor: undefined,
  conversationSession: undefined,
  userInteractionHandler: undefined,
  interruptionDetector: undefined,
  checkpointStateManager: undefined,
  workflowGraphRegistry: undefined,
  taskQueue: undefined,
  metricsRegistry: undefined,
  logger: undefined,
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

const createMockWorkflowExecutionBuilder = (): any => ({
  createChildExecution: () => Promise.resolve({ workflowExecutionEntity: {} }),
  buildExecutionPlan: () => ({}),
  validateExecutionPlan: () => true,
});

const createMockTaskQueue = (): any => ({
  enqueue: () => {},
  dequeue: () => undefined,
  peek: () => undefined,
  isEmpty: () => true,
  size: () => 0,
  clear: () => {},
  toArray: () => [],
});

const createMockWorkflowStateTransitor = (): any => ({
  transitionTo: () => Promise.resolve(),
  getCurrentState: () => 'RUNNING',
  canTransition: () => true,
  getValidTransitions: () => [],
});

describe('TriggerHandlerContextFactory', () => {
  let factory: TriggerHandlerContextFactory;
  let config: TriggerHandlerContextFactoryConfig;

  beforeEach(() => {
    config = {
      workflowExecutionRegistry: createMockWorkflowExecutionRegistry(),
      workflowRegistry: createMockWorkflowRegistry(),
      stateManager: createMockTriggerState(),
      globalContext: createMockGlobalContext(),
      checkpointStateManager: createMockCheckpointState(),
      graphRegistry: createMockWorkflowGraphRegistry(),
      eventManager: createMockEventRegistry(),
      executionBuilder: createMockWorkflowExecutionBuilder(),
      taskQueueManager: createMockTaskQueue(),
      workflowLifecycleCoordinator: createMockWorkflowStateTransitor(),
    };
    factory = new TriggerHandlerContextFactory(config);
  });

  describe('createHandlerContext', () => {
    it('should create lifecycle context for stop_workflow_execution', () => {
      const trigger = createMockTrigger('stop_workflow_execution');
      const result = factory.createHandlerContext(trigger);
      
      expect(result).toHaveProperty('workflowLifecycleCoordinator');
      expect((result as LifecycleTriggerContext).workflowLifecycleCoordinator).toBe(config.workflowLifecycleCoordinator);
    });

    it('should create lifecycle context for pause_workflow_execution', () => {
      const trigger = createMockTrigger('pause_workflow_execution');
      const result = factory.createHandlerContext(trigger);
      
      expect(result).toHaveProperty('workflowLifecycleCoordinator');
      expect((result as LifecycleTriggerContext).workflowLifecycleCoordinator).toBe(config.workflowLifecycleCoordinator);
    });

    it('should create lifecycle context for resume_workflow_execution', () => {
      const trigger = createMockTrigger('resume_workflow_execution');
      const result = factory.createHandlerContext(trigger);
      
      expect(result).toHaveProperty('workflowLifecycleCoordinator');
      expect((result as LifecycleTriggerContext).workflowLifecycleCoordinator).toBe(config.workflowLifecycleCoordinator);
    });

    it('should throw DependencyInjectionError when workflowLifecycleCoordinator is missing for lifecycle trigger', () => {
      // Remove workflowLifecycleCoordinator from config
      const configWithoutLifecycle = {
        ...config,
        workflowLifecycleCoordinator: undefined,
      };
      const factoryWithoutLifecycle = new TriggerHandlerContextFactory(configWithoutLifecycle);
      
      const trigger = createMockTrigger('stop_workflow_execution');
      
      expect(() => 
        factoryWithoutLifecycle.createHandlerContext(trigger)
      ).toThrow(DependencyInjectionError);
      expect(() => 
        factoryWithoutLifecycle.createHandlerContext(trigger)
      ).toThrow('WorkflowLifecycleCoordinator is required for lifecycle trigger actions');
    });

    it('should create skip node context for skip_node', () => {
      const trigger = createMockTrigger('skip_node');
      const result = factory.createHandlerContext(trigger);
      
      expect(result).toHaveProperty('workflowExecutionRegistry');
      expect(result).toHaveProperty('eventManager');
      expect((result as SkipNodeTriggerContext).workflowExecutionRegistry).toBe(config.workflowExecutionRegistry);
      expect((result as SkipNodeTriggerContext).eventManager).toBe(config.eventManager);
    });

    it('should throw DependencyInjectionError when eventManager is missing for skip_node trigger', () => {
      // Remove eventManager from config
      const configWithoutEventManager = {
        ...config,
        eventManager: undefined,
      };
      const factoryWithoutEventManager = new TriggerHandlerContextFactory(configWithoutEventManager);
      
      const trigger = createMockTrigger('skip_node');
      
      expect(() => 
        factoryWithoutEventManager.createHandlerContext(trigger)
      ).toThrow(DependencyInjectionError);
      expect(() => 
        factoryWithoutEventManager.createHandlerContext(trigger)
      ).toThrow('EventRegistry is required for skip_node trigger action');
    });

    it('should create set variable context for set_variable', () => {
      const trigger = createMockTrigger('set_variable');
      const result = factory.createHandlerContext(trigger);
      
      expect(result).toHaveProperty('workflowExecutionRegistry');
      expect((result as SetVariableTriggerContext).workflowExecutionRegistry).toBe(config.workflowExecutionRegistry);
    });

    it('should create subgraph context for execute_triggered_subworkflow', () => {
      const trigger = createMockTrigger('execute_triggered_subworkflow');
      const result = factory.createHandlerContext(trigger);
      
      expect(result).toHaveProperty('workflowExecutionRegistry');
      expect(result).toHaveProperty('eventManager');
      expect(result).toHaveProperty('executionBuilder');
      expect(result).toHaveProperty('taskQueueManager');
      expect(result).toHaveProperty('parentExecutionId');
      
      expect((result as ExecuteSubgraphTriggerContext).workflowExecutionRegistry).toBe(config.workflowExecutionRegistry);
      expect((result as ExecuteSubgraphTriggerContext).eventManager).toBe(config.eventManager);
      expect((result as ExecuteSubgraphTriggerContext).executionBuilder).toBe(config.executionBuilder);
      expect((result as ExecuteSubgraphTriggerContext).taskQueueManager).toBe(config.taskQueueManager);
      expect((result as ExecuteSubgraphTriggerContext).parentExecutionId).toBe('exec456');
    });

    it('should throw DependencyInjectionError when eventManager is missing for execute_triggered_subworkflow', () => {
      // Remove eventManager from config
      const configWithoutEventManager = {
        ...config,
        eventManager: undefined,
      };
      const factoryWithoutEventManager = new TriggerHandlerContextFactory(configWithoutEventManager);
      
      const trigger = createMockTrigger('execute_triggered_subworkflow');
      
      expect(() => 
        factoryWithoutEventManager.createHandlerContext(trigger)
      ).toThrow(DependencyInjectionError);
      expect(() => 
        factoryWithoutEventManager.createHandlerContext(trigger)
      ).toThrow('EventRegistry is required for execute_triggered_subworkflow trigger action');
    });

    it('should throw DependencyInjectionError when executionBuilder is missing for execute_triggered_subworkflow', () => {
      // Remove executionBuilder from config
      const configWithoutBuilder = {
        ...config,
        executionBuilder: undefined,
      };
      const factoryWithoutBuilder = new TriggerHandlerContextFactory(configWithoutBuilder);
      
      const trigger = createMockTrigger('execute_triggered_subworkflow');
      
      expect(() => 
        factoryWithoutBuilder.createHandlerContext(trigger)
      ).toThrow(DependencyInjectionError);
      expect(() => 
        factoryWithoutBuilder.createHandlerContext(trigger)
      ).toThrow('WorkflowExecutionBuilder is required for execute_triggered_subworkflow trigger action');
    });

    it('should throw DependencyInjectionError when taskQueueManager is missing for execute_triggered_subworkflow', () => {
      // Remove taskQueueManager from config
      const configWithoutTaskQueue = {
        ...config,
        taskQueueManager: undefined,
      };
      const factoryWithoutTaskQueue = new TriggerHandlerContextFactory(configWithoutTaskQueue);
      
      const trigger = createMockTrigger('execute_triggered_subworkflow');
      
      expect(() => 
        factoryWithoutTaskQueue.createHandlerContext(trigger)
      ).toThrow(DependencyInjectionError);
      expect(() => 
        factoryWithoutTaskQueue.createHandlerContext(trigger)
      ).toThrow('TaskQueue is required for execute_triggered_subworkflow trigger action');
    });

    it('should return empty context for unknown trigger actions', () => {
      const trigger = createMockTrigger('unknown_action');
      const result = factory.createHandlerContext(trigger);
      
      expect(result).toEqual({});
    });
  });

  describe('hasCheckpointSupport', () => {
    it('should return true when both checkpointStateManager and graphRegistry are present', () => {
      expect(factory.hasCheckpointSupport()).toBe(true);
    });

    it('should return false when checkpointStateManager is missing', () => {
      const configWithoutCheckpoint = {
        ...config,
        checkpointStateManager: undefined,
      };
      const factoryWithoutCheckpoint = new TriggerHandlerContextFactory(configWithoutCheckpoint);
      
      expect(factoryWithoutCheckpoint.hasCheckpointSupport()).toBe(false);
    });

    it('should return false when graphRegistry is missing', () => {
      const configWithoutGraph = {
        ...config,
        graphRegistry: undefined,
      };
      const factoryWithoutGraph = new TriggerHandlerContextFactory(configWithoutGraph);
      
      expect(factoryWithoutGraph.hasCheckpointSupport()).toBe(false);
    });

    it('should return false when both checkpointStateManager and graphRegistry are missing', () => {
      const configWithoutBoth = {
        ...config,
        checkpointStateManager: undefined,
        graphRegistry: undefined,
      };
      const factoryWithoutBoth = new TriggerHandlerContextFactory(configWithoutBoth);
      
      expect(factoryWithoutBoth.hasCheckpointSupport()).toBe(false);
    });
  });

  describe('getCheckpointStateManager', () => {
    it('should return checkpoint state manager when available', () => {
      expect(factory.getCheckpointStateManager()).toBe(config.checkpointStateManager);
    });

    it('should throw DependencyInjectionError when checkpointStateManager is missing', () => {
      const configWithoutCheckpoint = {
        ...config,
        checkpointStateManager: undefined,
      };
      const factoryWithoutCheckpoint = new TriggerHandlerContextFactory(configWithoutCheckpoint);
      
      expect(() => 
        factoryWithoutCheckpoint.getCheckpointStateManager()
      ).toThrow(DependencyInjectionError);
      expect(() => 
        factoryWithoutCheckpoint.getCheckpointStateManager()
      ).toThrow('CheckpointState is not configured');
    });
  });

  describe('getGraphRegistry', () => {
    it('should return graph registry when available', () => {
      expect(factory.getGraphRegistry()).toBe(config.graphRegistry);
    });

    it('should throw DependencyInjectionError when graphRegistry is missing', () => {
      const configWithoutGraph = {
        ...config,
        graphRegistry: undefined,
      };
      const factoryWithoutGraph = new TriggerHandlerContextFactory(configWithoutGraph);
      
      expect(() => 
        factoryWithoutGraph.getGraphRegistry()
      ).toThrow(DependencyInjectionError);
      expect(() => 
        factoryWithoutGraph.getGraphRegistry()
      ).toThrow('WorkflowGraphRegistry is not configured');
    });
  });

  describe('getStateManager', () => {
    it('should return trigger state manager', () => {
      expect(factory.getStateManager()).toBe(config.stateManager);
    });
  });

  describe('getGlobalContext', () => {
    it('should return global context', () => {
      expect(factory.getGlobalContext()).toBe(config.globalContext);
    });
  });

  describe('getDependencies', () => {
    it('should return the configuration object', () => {
      expect(factory.getDependencies()).toBe(config);
    });
  });
});