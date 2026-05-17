import { describe, it, expect, beforeEach, vi } from 'vitest';
import { WorkflowExecutionBuilder, type ChildExecutionOptions } from '../workflow-execution-builder.js';
import { ExecutionError, RuntimeValidationError } from '@wf-agent/types';
import type { WorkflowGraph } from '@wf-agent/types';
import type { GlobalContext } from '../../../../core/global-context.js';
import type { WorkflowRegistry } from '../../../stores/workflow-registry.js';
import type { WorkflowGraphRegistry } from '../../../stores/workflow-graph-registry.js';
import type { EventRegistry } from '../../../../core/registry/event-registry.js';
import type { ExecutionHierarchyRegistry } from '../../../../core/registry/execution-hierarchy-registry.js';

// Mock types for testing
interface MockGlobalContext extends GlobalContext {
  container: any;
  eventRegistry: EventRegistry;
}

interface MockWorkflowGraph extends WorkflowGraph {
  workflowId: string;
  workflowVersion: string;
  nodes: Map<string, any>;
  variables?: any[];
}

// Create mock implementations
const createMockGlobalContext = (): MockGlobalContext => {
  const container = {
    get: vi.fn(),
    set: vi.fn(),
    has: vi.fn(),
  };
  
  const eventRegistry = {
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
  } as any;
  
  return {
    container,
    eventRegistry,
  } as any;
};

const createMockWorkflowGraph = (workflowId: string = 'test-workflow'): WorkflowGraph => ({
  workflowId,
  workflowVersion: '1.0.0',
  nodes: new Map([
    ['start-node', { id: 'start-node', type: 'START', name: 'Start' }],
    ['end-node', { id: 'end-node', type: 'END', name: 'End' }],
  ]),
  edges: new Map(),
  variables: [],
} as any);

const createMockWorkflowRegistry = (): WorkflowRegistry => ({
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
} as any);

const createMockWorkflowGraphRegistry = (): WorkflowGraphRegistry => ({
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
} as any);

const createMockExecutionHierarchyRegistry = (): ExecutionHierarchyRegistry => ({
  executions: new Map(),
  register: () => {},
  get: () => undefined,
  delete: () => {},
  getAll: () => [],
  getAllIds: () => [],
  size: () => 0,
  clear: () => {},
  has: () => false,
} as any);

const createMockVariableCoordinator = () => ({
  initializeFromDefinitions: vi.fn(),
});

describe('WorkflowExecutionBuilder', () => {
  let builder: WorkflowExecutionBuilder;
  let globalContext: MockGlobalContext;
  let workflowGraphRegistry: WorkflowGraphRegistry;
  let workflowRegistry: WorkflowRegistry;
  let executionHierarchyRegistry: ExecutionHierarchyRegistry;
  let variableCoordinator: any;

  beforeEach(() => {
    globalContext = createMockGlobalContext();
    workflowGraphRegistry = createMockWorkflowGraphRegistry();
    workflowRegistry = createMockWorkflowRegistry();
    executionHierarchyRegistry = createMockExecutionHierarchyRegistry();
    variableCoordinator = createMockVariableCoordinator();
    
    // Setup DI container mocks
    globalContext.container.get.mockImplementation((identifier: any) => {
      if (identifier.toString().includes('WorkflowGraphRegistry')) {
        return workflowGraphRegistry;
      }
      if (identifier.toString().includes('WorkflowRegistry')) {
        return workflowRegistry;
      }
      if (identifier.toString().includes('ExecutionHierarchyRegistry')) {
        return executionHierarchyRegistry;
      }
      if (identifier.toString().includes('VariableCoordinator')) {
        return variableCoordinator;
      }
      return undefined;
    });
    
    builder = new WorkflowExecutionBuilder(globalContext);
  });

  describe('build', () => {
    it('should build workflow execution entity from workflow graph', async () => {
      const workflowId = 'test-workflow';
      const mockGraph = createMockWorkflowGraph(workflowId);
      
      // Mock the workflow graph registry to return our mock graph
      workflowGraphRegistry.get = vi.fn().mockReturnValue(mockGraph);
      
      const result = await builder.build(workflowId);
      
      expect(result).toHaveProperty('workflowExecutionEntity');
      expect(result).toHaveProperty('stateCoordinator');
      expect(result).toHaveProperty('conversationManager');
      expect(result.workflowExecutionEntity).toBeDefined();
      expect(result.stateCoordinator).toBeDefined();
      expect(result.conversationManager).toBeDefined();
    });

    it('should throw ExecutionError when workflow graph is not found', async () => {
      const workflowId = 'non-existent-workflow';
      
      // Mock the workflow graph registry to return undefined
      workflowGraphRegistry.get = vi.fn().mockReturnValue(undefined);
      
      await expect(builder.build(workflowId)).rejects.toThrow(ExecutionError);
      await expect(builder.build(workflowId)).rejects.toThrow(`Workflow '${workflowId}' not found or not preprocessed`);
    });

    it('should throw RuntimeValidationError when workflow graph has no nodes', async () => {
      const workflowId = 'empty-workflow';
      const emptyGraph = {
        ...createMockWorkflowGraph(workflowId),
        nodes: new Map(),
      };
      
      workflowGraphRegistry.get = vi.fn().mockReturnValue(emptyGraph);
      
      await expect(builder.build(workflowId)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.build(workflowId)).rejects.toThrow('Workflow graph must have at least one node');
    });

    it('should throw RuntimeValidationError when workflow graph has no START node', async () => {
      const workflowId = 'no-start-workflow';
      const noStartGraph = {
        ...createMockWorkflowGraph(workflowId),
        nodes: new Map([
          ['end-node', { id: 'end-node', type: 'END', name: 'End' }],
        ]),
      };
      
      workflowGraphRegistry.get = vi.fn().mockReturnValue(noStartGraph);
      
      await expect(builder.build(workflowId)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.build(workflowId)).rejects.toThrow('Workflow graph must have a START node');
    });

    it('should throw RuntimeValidationError when workflow graph has no END node', async () => {
      const workflowId = 'no-end-workflow';
      const noEndGraph = {
        ...createMockWorkflowGraph(workflowId),
        nodes: new Map([
          ['start-node', { id: 'start-node', type: 'START', name: 'Start' }],
        ]),
      };
      
      workflowGraphRegistry.get = vi.fn().mockReturnValue(noEndGraph);
      
      await expect(builder.build(workflowId)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.build(workflowId)).rejects.toThrow('Workflow graph must have an END node');
    });
  });

  describe('buildFromTemplate', () => {
    it('should throw RuntimeValidationError when template is not found', async () => {
      await expect(builder.buildFromTemplate('non-existent-template')).rejects.toThrow(RuntimeValidationError);
      await expect(builder.buildFromTemplate('non-existent-template')).rejects.toThrow('Workflow execution template not found: non-existent-template');
    });
  });

  describe('createChildExecution', () => {
    it('should validate SUBGRAPH configuration', async () => {
      const parentEntity = {
        id: 'parent-exec-123',
        getWorkflowExecutionData: () => ({
          id: 'parent-exec-123',
          workflowId: 'parent-workflow',
          variableScopes: {
            global: {},
            execution: {},
          },
        }),
        getHierarchyMetadata: () => undefined,
        getRootExecutionId: () => '',
        getRootExecutionType: () => '',
        messageHistoryManager: {
          getMessages: () => [],
        },
        variableStateManager: {
          copyFrom: () => {},
          importVariables: () => {},
        },
        setParentContext: () => {},
        registerChild: () => {},
      } as any;
      
      const options: ChildExecutionOptions = {
        type: 'SUBGRAPH',
        config: {
          // Missing subworkflowId - should throw error
        },
      };
      
      await expect(builder.createChildExecution(parentEntity, options)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.createChildExecution(parentEntity, options)).rejects.toThrow('SUBGRAPH requires subworkflowId');
    });

    it('should validate FORK_BRANCH configuration', async () => {
      const parentEntity = {
        id: 'parent-exec-123',
        getWorkflowExecutionData: () => ({
          id: 'parent-exec-123',
          workflowId: 'parent-workflow',
          variableScopes: {
            global: {},
            execution: {},
          },
        }),
        getHierarchyMetadata: () => undefined,
        getRootExecutionId: () => '',
        getRootExecutionType: () => '',
        messageHistoryManager: {
          getMessages: () => [],
        },
        variableStateManager: {
          copyFrom: () => {},
          importVariables: () => {},
        },
        setParentContext: () => {},
        registerChild: () => {},
      } as any;
      
      const options: ChildExecutionOptions = {
        type: 'FORK_BRANCH',
        config: {
          // Missing forkPathId - should throw error
        },
      };
      
      await expect(builder.createChildExecution(parentEntity, options)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.createChildExecution(parentEntity, options)).rejects.toThrow('FORK_BRANCH requires forkPathId');
    });

    it('should validate TRIGGERED configuration', async () => {
      const parentEntity = {
        id: 'parent-exec-123',
        getWorkflowExecutionData: () => ({
          id: 'parent-exec-123',
          workflowId: 'parent-workflow',
          variableScopes: {
            global: {},
            execution: {},
          },
        }),
        getHierarchyMetadata: () => undefined,
        getRootExecutionId: () => '',
        getRootExecutionType: () => '',
        messageHistoryManager: {
          getMessages: () => [],
        },
        variableStateManager: {
          copyFrom: () => {},
          importVariables: () => {},
        },
        setParentContext: () => {},
        registerChild: () => {},
      } as any;
      
      const options: ChildExecutionOptions = {
        type: 'TRIGGERED',
        config: {
          // Missing subworkflowId - should throw error
        },
      };
      
      await expect(builder.createChildExecution(parentEntity, options)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.createChildExecution(parentEntity, options)).rejects.toThrow('TRIGGERED requires subworkflowId');
    });
  });

  describe('clearCache', () => {
    it('should clear the workflow execution templates cache', () => {
      // This test verifies that the clearCache method exists and can be called
      expect(() => builder.clearCache()).not.toThrow();
    });
  });

  describe('invalidateWorkflow', () => {
    it('should invalidate cache for specified workflow', () => {
      // This test verifies that the invalidateWorkflow method exists and can be called
      expect(() => builder.invalidateWorkflow('test-workflow')).not.toThrow();
    });
  });
});
