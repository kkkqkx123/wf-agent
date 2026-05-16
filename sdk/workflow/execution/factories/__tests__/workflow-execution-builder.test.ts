import { describe, it, expect, beforeEach } from 'vitest';
import { WorkflowExecutionBuilder, WorkflowExecutionBuildResult } from '../workflow-execution-builder';
import { ExecutionError, RuntimeValidationError } from '@wf-agent/types';
import { generateId } from '@wf-agent/common-utils';

// Mock types for testing
interface MockGlobalContext {}
interface MockWorkflowGraphRegistry {}
interface MockWorkflowRegistry {}
interface MockEventRegistry {}
interface MockExecutionHierarchyRegistry {}
interface MockVariableCoordinator {}
interface MockWorkflowGraph {}
interface MockWorkflowExecution {}
interface MockVariableDefinition {}

// Create mock implementations
const createMockGlobalContext = (): MockGlobalContext => ({
  container: {
    get: jest.fn()
  },
  eventRegistry: {} as MockEventRegistry
} as MockGlobalContext);

const createMockWorkflowGraphRegistry = (): MockWorkflowGraphRegistry => ({
  get: jest.fn()
} as MockWorkflowGraphRegistry);

const createMockWorkflowRegistry = (): MockWorkflowRegistry => ({
  get: jest.fn()
} as MockWorkflowRegistry);

const createMockEventRegistry = (): MockEventRegistry => ({} as MockEventRegistry);

const createMockExecutionHierarchyRegistry = (): MockExecutionHierarchyRegistry => ({
  register: jest.fn()
} as MockExecutionHierarchyRegistry);

const createMockVariableCoordinator = (): MockVariableCoordinator => ({
  initializeFromDefinitions: jest.fn()
} as MockVariableCoordinator);

const createMockWorkflowGraph = (workflowId: string = 'workflow123'): MockWorkflowGraph => ({
  workflowId,
  workflowVersion: '1.0',
  nodes: new Map([
    ['start', { id: 'start', type: 'START' }],
    ['end', { id: 'end', type: 'END' }]
  ]),
  variables: [] as MockVariableDefinition[]
} as MockWorkflowGraph);

const createMockWorkflowExecution = (): MockWorkflowExecution => ({
  id: 'exec123',
  workflowId: 'workflow123',
  workflowVersion: '1.0',
  currentNodeId: 'start',
  graph: {} as MockWorkflowGraph,
  variables: [],
  variableScopes: {
    global: {},
    execution: {}
  },
  input: {},
  output: {},
  nodeResults: [],
  errors: [],
  executionType: 'MAIN'
} as MockWorkflowExecution);

const createMockVariableDefinition = (): MockVariableDefinition => ({
  name: 'testVar',
  type: 'string',
  scope: 'execution'
} as MockVariableDefinition);

describe('WorkflowExecutionBuilder', () => {
  let builder: WorkflowExecutionBuilder;
  let globalContext: MockGlobalContext;
  let workflowGraphRegistry: MockWorkflowGraphRegistry;
  let workflowRegistry: MockWorkflowRegistry;
  let eventRegistry: MockEventRegistry;
  let executionHierarchyRegistry: MockExecutionHierarchyRegistry;
  let variableCoordinator: MockVariableCoordinator;

  beforeEach(() => {
    // Create mock dependencies
    globalContext = createMockGlobalContext();
    workflowGraphRegistry = createMockWorkflowGraphRegistry();
    workflowRegistry = createMockWorkflowRegistry();
    eventRegistry = createMockEventRegistry();
    executionHierarchyRegistry = createMockExecutionHierarchyRegistry();
    variableCoordinator = createMockVariableCoordinator();
    
    // Set up DI container mock
    globalContext.container.get = jest.fn();
    globalContext.container.get
      .mockReturnValueOnce(workflowGraphRegistry) // WorkflowGraphRegistry
      .mockReturnValueOnce(variableCoordinator) // VariableCoordinator
      .mockReturnValueOnce(eventRegistry) // EventRegistry
      .mockReturnValueOnce(workflowRegistry) // WorkflowRegistry
      .mockReturnValueOnce(executionHierarchyRegistry); // ExecutionHierarchyRegistry
    
    globalContext.eventRegistry = eventRegistry;
    
    // Create builder with globalContext
    builder = new WorkflowExecutionBuilder(globalContext);
  });

  describe('constructor', () => {
    it('should initialize with empty template map when globalContext is provided', () => {
      const builderWithGlobalContext = new WorkflowExecutionBuilder(globalContext);
      expect(builderWithGlobalContext['workflowExecutionTemplates']).toBeInstanceOf(Map);
      expect(builderWithGlobalContext['workflowExecutionTemplates'].size).toBe(0);
    });

    it('should initialize with empty template map when globalContext is not provided', () => {
      const builderWithoutGlobalContext = new WorkflowExecutionBuilder();
      expect(builderWithoutGlobalContext['workflowExecutionTemplates']).toBeInstanceOf(Map);
      expect(builderWithoutGlobalContext['workflowExecutionTemplates'].size).toBe(0);
    });
  });

  describe('build', () => {
    it('should build workflow execution from workflowId', async () => {
      const workflowId = 'workflow123';
      const workflowGraph = createMockWorkflowGraph(workflowId);
      
      // Mock get method to return workflow graph
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(workflowGraph);
      
      // Mock workflow registry get method
      (workflowRegistry.get as jest.Mock).mockReturnValue({ config: {} });
      
      const result = await builder.build(workflowId);
      
      expect(result).toHaveProperty('workflowExecutionEntity');
      expect(result).toHaveProperty('stateCoordinator');
      expect(result).toHaveProperty('conversationManager');
      
      // Verify workflow graph was retrieved
      expect(workflowGraphRegistry.get).toHaveBeenCalledWith(workflowId);
    });

    it('should throw ExecutionError when workflow is not found', async () => {
      const workflowId = 'nonexistent-workflow';
      
      // Mock get method to return undefined
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(undefined);
      
      await expect(builder.build(workflowId)).rejects.toThrow(ExecutionError);
      await expect(builder.build(workflowId)).rejects.toThrow(`Workflow '${workflowId}' not found or not preprocessed`);
    });

    it('should throw RuntimeValidationError when workflow has no nodes', async () => {
      const workflowId = 'workflow123';
      const workflowGraph = createMockWorkflowGraph(workflowId);
      
      // Clear nodes
      workflowGraph.nodes.clear();
      
      // Mock get method to return workflow graph
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(workflowGraph);
      
      await expect(builder.build(workflowId)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.build(workflowId)).rejects.toThrow('Workflow graph must have at least one node');
    });

    it('should throw RuntimeValidationError when workflow has no START node', async () => {
      const workflowId = 'workflow123';
      const workflowGraph = createMockWorkflowGraph(workflowId);
      
      // Remove START node
      workflowGraph.nodes.delete('start');
      
      // Mock get method to return workflow graph
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(workflowGraph);
      
      await expect(builder.build(workflowId)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.build(workflowId)).rejects.toThrow('Workflow graph must have a START node');
    });

    it('should throw RuntimeValidationError when workflow has no END node', async () => {
      const workflowId = 'workflow123';
      const workflowGraph = createMockWorkflowGraph(workflowId);
      
      // Remove END node
      workflowGraph.nodes.delete('end');
      
      // Mock get method to return workflow graph
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(workflowGraph);
      
      await expect(builder.build(workflowId)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.build(workflowId)).rejects.toThrow('Workflow graph must have an END node');
    });

    it('should initialize variables from workflow graph definitions', async () => {
      const workflowId = 'workflow123';
      const workflowGraph = createMockWorkflowGraph(workflowId);
      const variableDef = createMockVariableDefinition();
      workflowGraph.variables = [variableDef];
      
      // Mock get method to return workflow graph
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(workflowGraph);
      
      // Mock workflow registry get method
      (workflowRegistry.get as jest.Mock).mockReturnValue({ config: {} });
      
      await builder.build(workflowId);
      
      // Verify initializeFromDefinitions was called with the variable definition
      expect(variableCoordinator.initializeFromDefinitions).toHaveBeenCalledWith(
        expect.anything(), // VariableManager
        [variableDef]
      );
    });
  });

  describe('buildFromTemplate', () => {
    it('should build workflow execution from cached template', async () => {
      // Create a template
      const templateId = 'template123';
      const template = createMockWorkflowExecution();
      
      // Add template to cache
      const builderWithTemplate = new WorkflowExecutionBuilder(globalContext);
      // @ts-ignore - Access private property for testing
      builderWithTemplate['workflowExecutionTemplates'].set(templateId, template);
      
      const result = await builderWithTemplate.buildFromTemplate(templateId);
      
      expect(result).toHaveProperty('workflowExecutionEntity');
      expect(result).toHaveProperty('stateCoordinator');
      expect(result).toHaveProperty('conversationManager');
    });

    it('should throw RuntimeValidationError when template is not found', async () => {
      const templateId = 'nonexistent-template';
      
      await expect(builder.buildFromTemplate(templateId)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.buildFromTemplate(templateId)).rejects.toThrow(`Workflow execution template not found: ${templateId}`);
    });
  });

  describe('createCopy', () => {
    it('should create a copy of workflow execution entity', async () => {
      // Create source entity
      const sourceWorkflowExecution = createMockWorkflowExecution();
      const sourceWorkflowExecutionEntity = {
        getWorkflowExecutionData: () => sourceWorkflowExecution,
        variableStateManager: {
          copyFrom: jest.fn()
        },
        messageHistoryManager: {
          getMessages: () => []
        }
      } as any;
      
      const result = await builder.createCopy(sourceWorkflowExecutionEntity);
      
      expect(result).toHaveProperty('workflowExecutionEntity');
      expect(result).toHaveProperty('stateCoordinator');
      expect(result).toHaveProperty('conversationManager');
      
      // Verify the copied execution has a new ID
      expect(result.workflowExecutionEntity.id).not.toBe(sourceWorkflowExecution.id);
      
      // Verify variable state was copied
      expect(sourceWorkflowExecutionEntity.variableStateManager.copyFrom).toHaveBeenCalled();
    });
  });

  describe('createFork', () => {
    it('should create a fork of workflow execution entity', async () => {
      // Create parent entity
      const parentWorkflowExecution = createMockWorkflowExecution();
      const parentWorkflowExecutionEntity = {
        getWorkflowExecutionData: () => parentWorkflowExecution,
        variableStateManager: {
          copyFrom: jest.fn()
        },
        messageHistoryManager: {
          getMessages: () => []
        }
      } as any;
      
      const forkConfig = { forkId: 'fork1', forkPathId: 'path1' };
      
      const result = await builder.createFork(parentWorkflowExecutionEntity, forkConfig);
      
      expect(result).toHaveProperty('workflowExecutionEntity');
      expect(result).toHaveProperty('stateCoordinator');
      expect(result).toHaveProperty('conversationManager');
      
      // Verify the fork execution has a new ID
      expect(result.workflowExecutionEntity.id).not.toBe(parentWorkflowExecution.id);
      
      // Verify fork context is set correctly
      expect((result.workflowExecutionEntity.getWorkflowExecutionData() as any).forkJoinContext).toEqual({
        forkId: 'fork1',
        forkPathId: 'path1'
      });
      
      // Verify variable state was copied
      expect(parentWorkflowExecutionEntity.variableStateManager.copyFrom).toHaveBeenCalled();
    });
  });

  describe('createSubgraph', () => {
    it('should create a subgraph execution entity', async () => {
      // Create parent entity
      const parentWorkflowExecution = createMockWorkflowExecution();
      const parentWorkflowExecutionEntity = {
        getWorkflowExecutionData: () => parentWorkflowExecution,
        variableStateManager: {
          copyFrom: jest.fn(),
          importVariables: jest.fn()
        },
        messageHistoryManager: {
          getMessages: () => []
        },
        id: 'parent-exec-id',
        getHierarchyMetadata: () => ({ depth: 1 }),
        getRootExecutionId: () => 'root-exec-id',
        getRootExecutionType: () => 'MAIN',
        registerChild: jest.fn()
      } as any;
      
      // Create subgraph workflow
      const subgraphWorkflowId = 'subgraph123';
      const subgraphWorkflowGraph = createMockWorkflowGraph(subgraphWorkflowId);
      
      // Mock get method to return subgraph workflow
      (workflowGraphRegistry.get as jest.Mock)
        .mockReturnValueOnce(subgraphWorkflowGraph) // For subgraph
        .mockReturnValueOnce(parentWorkflowExecution.graph); // For parent
      
      // Mock variable coordinator
      const variableCoordinator = createMockVariableCoordinator();
      globalContext.container.get = jest.fn();
      globalContext.container.get
        .mockReturnValueOnce(workflowGraphRegistry) // WorkflowGraphRegistry
        .mockReturnValueOnce(variableCoordinator) // VariableCoordinator
        .mockReturnValueOnce(eventRegistry) // EventRegistry
        .mockReturnValueOnce(workflowRegistry) // WorkflowRegistry
        .mockReturnValueOnce(executionHierarchyRegistry); // ExecutionHierarchyRegistry
      
      const options = {
        subworkflowId: subgraphWorkflowId,
        nodeId: 'subgraph-node',
        variableMapping: {
          inputs: [{ externalName: 'input1', internalName: 'internal1' }]
        }
      };
      
      const result = await builder.createSubgraph(parentWorkflowExecutionEntity, options);
      
      expect(result).toHaveProperty('workflowExecutionEntity');
      expect(result).toHaveProperty('stateCoordinator');
      expect(result).toHaveProperty('conversationManager');
      
      // Verify subgraph workflow was retrieved
      expect(workflowGraphRegistry.get).toHaveBeenCalledWith(subgraphWorkflowId);
      
      // Verify variable import was called
      expect(parentWorkflowExecutionEntity.variableStateManager.importVariables).toHaveBeenCalled();
      
      // Verify variable initialization was called
      expect(variableCoordinator.initializeFromDefinitions).toHaveBeenCalled();
      
      // Verify hierarchy registration
      expect(executionHierarchyRegistry.register).toHaveBeenCalled();
      expect(parentWorkflowExecutionEntity.registerChild).toHaveBeenCalled();
    });

    it('should throw ExecutionError when subworkflow is not found', async () => {
      // Create parent entity
      const parentWorkflowExecution = createMockWorkflowExecution();
      const parentWorkflowExecutionEntity = {
        getWorkflowExecutionData: () => parentWorkflowExecution,
        variableStateManager: {
          copyFrom: jest.fn()
        },
        messageHistoryManager: {
          getMessages: () => []
        }
      } as any;
      
      // Mock get method to return undefined for subworkflow
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(undefined);
      
      const options = {
        subworkflowId: 'nonexistent-subworkflow',
        nodeId: 'subgraph-node'
      };
      
      await expect(builder.createSubgraph(parentWorkflowExecutionEntity, options)).rejects.toThrow(ExecutionError);
      await expect(builder.createSubgraph(parentWorkflowExecutionEntity, options)).rejects.toThrow('Subworkflow');
    });

    it('should throw RuntimeValidationError when subworkflow has no START node', async () => {
      // Create parent entity
      const parentWorkflowExecution = createMockWorkflowExecution();
      const parentWorkflowExecutionEntity = {
        getWorkflowExecutionData: () => parentWorkflowExecution,
        variableStateManager: {
          copyFrom: jest.fn()
        },
        messageHistoryManager: {
          getMessages: () => []
        }
      } as any;
      
      // Create subgraph workflow without START node
      const subgraphWorkflowId = 'subgraph123';
      const subgraphWorkflowGraph = createMockWorkflowGraph(subgraphWorkflowId);
      subgraphWorkflowGraph.nodes.clear(); // Remove all nodes
      
      // Mock get method to return subgraph workflow
      (workflowGraphRegistry.get as jest.Mock).mockReturnValue(subgraphWorkflowGraph);
      
      const options = {
        subworkflowId: subgraphWorkflowId,
        nodeId: 'subgraph-node'
      };
      
      await expect(builder.createSubgraph(parentWorkflowExecutionEntity, options)).rejects.toThrow(RuntimeValidationError);
      await expect(builder.createSubgraph(parentWorkflowExecutionEntity, options)).rejects.toThrow('Subworkflow graph must have a START node');
    });
  });

  describe('clearCache', () => {
    it('should clear the workflow execution templates cache', () => {
      // Add a template to cache
      const templateId = 'template123';
      const template = createMockWorkflowExecution();
      
      // @ts-ignore - Access private property for testing
      builder['workflowExecutionTemplates'].set(templateId, template);
      
      // Verify template is in cache
      expect(builder['workflowExecutionTemplates'].size).toBe(1);
      
      // Clear cache
      builder.clearCache();
      
      // Verify cache is empty
      expect(builder['workflowExecutionTemplates'].size).toBe(0);
    });
  });

  describe('invalidateWorkflow', () => {
    it('should invalidate cache for specified workflow', () => {
      // Add templates to cache
      const template1 = createMockWorkflowExecution();
      template1.workflowId = 'workflow1';
      
      const template2 = createMockWorkflowExecution();
      template2.workflowId = 'workflow2';
      
      const template3 = createMockWorkflowExecution();
      template3.workflowId = 'workflow1';
      
      // @ts-ignore - Access private property for testing
      builder['workflowExecutionTemplates'].set('template1', template1);
      builder['workflowExecutionTemplates'].set('template2', template2);
      builder['workflowExecutionTemplates'].set('template3', template3);
      
      // Verify templates are in cache
      expect(builder['workflowExecutionTemplates'].size).toBe(3);
      
      // Invalidate workflow1
      builder.invalidateWorkflow('workflow1');
      
      // Verify only workflow1 templates are removed
      expect(builder['workflowExecutionTemplates'].size).toBe(1);
      expect(builder['workflowExecutionTemplates'].has('template2')).toBe(true);
      expect(builder['workflowExecutionTemplates'].has('template1')).toBe(false);
      expect(builder['workflowExecutionTemplates'].has('template3')).toBe(false);
    });
  });
});