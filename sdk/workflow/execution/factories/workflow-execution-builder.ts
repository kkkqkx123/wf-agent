/**
 * WorkflowExecutionBuilder - Workflow Execution Builder
 *
 * Responsible for retrieving WorkflowTemplate from WorkflowRegistry and creating WorkflowExecutionEntity instances.
 * Provides support for workflow execution template caching and deep copying, as well as the use of preprocessed graphs and graph navigation.
 *
 * Note: WorkflowExecutionBuilder is used directly for workflow execution creation, not wrapped as WorkflowExecutionFactory.
 * The naming difference from AgentLoopFactory reflects the different execution models.
 *
 * Migration Note:
 * - WorkflowExecutionEntity no longer holds ConversationManager directly
 * - WorkflowStateCoordinator is created separately to manage state
 * - This eliminates data redundancy and synchronization issues
 */

import type { WorkflowGraph, WorkflowExecution, WorkflowExecutionOptions, WorkflowConfig, MessageContextRegistry, VariableDefinition } from "@wf-agent/types";
import { AvailableTools, resolveSchemaTools, resolveInitialTools } from "@wf-agent/types";
import { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import { WorkflowExecutionState } from "../../state-managers/workflow-execution-state.js";
import { ExecutionState } from "../../state-managers/execution-state.js";
import { WorkflowStateCoordinator } from "../../state-managers/workflow-state-coordinator.js";
import { generateId } from "@wf-agent/common-utils";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";
import type { WorkflowGraphRegistry } from "../../stores/workflow-graph-registry.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { WorkflowRegistry } from "../../stores/workflow-registry.js";
import { ConversationSession } from "../../../core/messaging/conversation-session.js";
import { InMemoryMessageContextRegistry, initializeExecutionContext } from "../../../core/messaging/index.js";
import { logError, emitErrorEvent } from "../../../core/utils/error-utils.js";
import type { ExecutionHierarchyRegistry } from "../../../core/registry/execution-hierarchy-registry.js";
import type { GlobalContext } from "../../../core/global-context.js";
import type { VariableManager } from "../../state-managers/variable-manager.js";
import { ToolPermissionManager } from "../../../core/coordinators/tool-permission-manager.js";

const logger = createContextualLogger({ operation: "workflow-execution-builder" });

/**
 * Workflow Execution Build Result
 * Contains both WorkflowExecutionEntity and WorkflowStateCoordinator
 */
export interface WorkflowExecutionBuildResult {
  workflowExecutionEntity: WorkflowExecutionEntity;
  stateCoordinator: WorkflowStateCoordinator;
  conversationManager: ConversationSession;
}

/**
 * Child Execution Type - Unified type for all child execution scenarios
 */
export type ChildExecutionType = 'SUBGRAPH' | 'FORK_BRANCH' | 'TRIGGERED';

/**
 * Child Execution Configuration
 */
export interface ChildExecutionConfig {
  subworkflowId?: string;      // SUBGRAPH/TRIGGERED
  forkPathId?: string;         // FORK_BRANCH
  startNodeId?: string;        // FORK_BRANCH
  nodeId?: string;             // SUBGRAPH (node ID in parent)
  variableMapping?: {
    inputs?: Array<{ externalName: string; internalName: string; required?: boolean; defaultValue?: unknown }>;
    outputs?: Array<{ internalName: string; externalName: string }>;
  };
  /**
   * Data Mapping Configuration
   *
   * Maps parent execution input data fields to child execution variables.
   * This enables explicit data passing from the parent's WorkflowExecution.input
   * to the child's variable system, without relying on implicit inheritance.
   *
   * The mapping is processed during child execution initialization:
   * - Reads parentField from parent's WorkflowExecution.input
   * - Sets childVariableName in child's VariableManager
   * - Validates required fields and applies default values
   *
   * Note: This is different from variableMapping.inputs which maps parent
   * variables to child variables. dataMapping maps execution-level input
   * data to variables.
   */
  dataMapping?: {
    inputs?: Array<{
      parentField: string;         // key in parent's WorkflowExecution.input
      childVariableName: string;   // variable name in child's VariableManager
      required?: boolean;
      defaultValue?: unknown;
    }>;
  };
  inputMapping?: any;          // TRIGGERED - ExecuteTriggeredSubworkflowActionConfig['inputMapping']
  async?: boolean;             // TRIGGERED
}

/**
 * Child Execution Options
 */
export interface ChildExecutionOptions {
  type: ChildExecutionType;
  config: ChildExecutionConfig;
}

/**
 * WorkflowExecutionBuilder - Workflow Execution Builder
 *
 * Factory class for creating WorkflowExecutionEntity instances from workflows.
 */
export class WorkflowExecutionBuilder {
  private workflowExecutionTemplates: Map<string, WorkflowExecutionEntity> = new Map();
  private globalContext?: GlobalContext;

  constructor(globalContext?: GlobalContext) {
    this.globalContext = globalContext;
  }

  /**
   * Get workflow graph registry (from DI container)
   */
  private getWorkflowGraphRegistry(): WorkflowGraphRegistry {
    if (!this.globalContext) {
      throw new Error("GlobalContext not initialized. Use constructor with GlobalContext parameter.");
    }
    return this.globalContext.container.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
  }

  /**
   * Get variable coordinator (from DI container)
   */
  private getVariableCoordinator(): unknown {
    if (!this.globalContext) {
      throw new Error("GlobalContext not initialized. Use constructor with GlobalContext parameter.");
    }
    return this.globalContext.container.get(Identifiers.VariableCoordinator);
  }

  /**
   * Get event manager (from DI container)
   */
  private getEventManager(): EventRegistry {
    if (!this.globalContext) {
      throw new Error("GlobalContext not initialized. Use constructor with GlobalContext parameter.");
    }
    return this.globalContext.eventRegistry;
  }

  /**
   * Get workflow registry (from DI container)
   */
  private getWorkflowRegistry(): WorkflowRegistry {
    if (!this.globalContext) {
      throw new Error("GlobalContext not initialized. Use constructor with GlobalContext parameter.");
    }
    return this.globalContext.container.get(Identifiers.WorkflowRegistry) as WorkflowRegistry;
  }

  /**
   * Get execution hierarchy registry (from DI container)
   */
  private getExecutionHierarchyRegistry(): ExecutionHierarchyRegistry {
    if (!this.globalContext) {
      throw new Error("GlobalContext not initialized. Use constructor with GlobalContext parameter.");
    }
    return this.globalContext.container.get(Identifiers.ExecutionHierarchyRegistry) as ExecutionHierarchyRegistry;
  }

  /**
   * Get workflow from WorkflowRegistry and construct WorkflowExecutionEntity
   *
   * @param workflowId Workflow ID
   * @param options Workflow execution options
   * @returns WorkflowExecutionBuildResult containing WorkflowExecutionEntity, WorkflowStateCoordinator, and ConversationSession
   */
  async build(
    workflowId: string,
    options: WorkflowExecutionOptions = {},
  ): Promise<WorkflowExecutionBuildResult> {
    logger.info("Building workflow execution from workflow", { workflowId });

    // Get workflow graph from workflow-graph-registry
    const workflowGraph = this.getWorkflowGraphRegistry().get(workflowId);

    if (!workflowGraph) {
      const error = new ExecutionError(
        `Workflow '${workflowId}' not found or not preprocessed`,
        undefined,
        workflowId,
      );
      const eventManager = this.getEventManager();
      logError(error, { workflowId });
      emitErrorEvent(eventManager, {
        executionId: "",
        workflowId,
        error,
      });
      throw error;
    }

    // Build from WorkflowGraph
    const result = await this.buildFromWorkflowGraph(workflowGraph, options, workflowId);

    logger.info("Workflow execution built successfully", {
      workflowExecutionId: result.workflowExecutionEntity.id,
      workflowId,
    });

    return result;
  }

  /**
   * Build WorkflowExecutionEntity from WorkflowGraph (internal method)
   *
   * @param workflowGraph Workflow graph
   * @param options Workflow execution options
   * @returns WorkflowExecutionBuildResult containing WorkflowExecutionEntity, WorkflowStateCoordinator, and ConversationSession
   */
  private async buildFromWorkflowGraph(
    workflowGraph: WorkflowGraph,
    options: WorkflowExecutionOptions = {},
    workflowId?: string,
  ): Promise<WorkflowExecutionBuildResult> {
    // Step 1: Verify workflow graph
    if (!workflowGraph.nodes || workflowGraph.nodes.size === 0) {
      throw new RuntimeValidationError("Workflow graph must have at least one node", {
        field: "graph.nodes",
      });
    }

    const startNode = Array.from(workflowGraph.nodes.values()).find(n => n.type === "START");
    if (!startNode) {
      throw new RuntimeValidationError("Workflow graph must have a START node", {
        field: "graph.nodes",
      });
    }

    const endNode = Array.from(workflowGraph.nodes.values()).find(n => n.type === "END");
    if (!endNode) {
      throw new RuntimeValidationError("Workflow graph must have an END node", {
        field: "graph.nodes",
      });
    }

    // Step 2: WorkflowGraph itself is a Graph containing complete graph structure
    const workflowExecutionGraphData = workflowGraph;

    // Step 3: Create WorkflowExecution instance
    const executionId = generateId();

    const workflowExecution: WorkflowExecution = {
      id: executionId,
      workflowId: workflowGraph.workflowId,
      workflowVersion: workflowGraph.workflowVersion,
      currentNodeId: startNode.id,
      graph: workflowExecutionGraphData,
      variables: [],
      input: options.input || {},
      output: {},
      nodeResults: [],
      errors: [],
      executionType: "MAIN",
    };

    // Step 4: Create WorkflowExecutionState
    const workflowExecutionState = new WorkflowExecutionState();

    // Step 5: Create ExecutionState (for subgraph stack management)
    const executionState = new ExecutionState();

    // Step 6: Create WorkflowExecutionEntity
    // Note: WorkflowExecutionEntity internally creates its own VariableManager instance
    // This follows the same pattern as MessageHistory - each entity owns its state managers
    const registry = this.getExecutionHierarchyRegistry();
    const workflowExecutionEntity = new WorkflowExecutionEntity(
      workflowExecution,
      executionState,
      workflowExecutionState,
      undefined,
      registry
    );

    // Step 7: Initialize variables in the entity's VariableManager
    // The VariableCoordinator is stateless and receives the manager as a parameter
    const variableCoordinator = this.getVariableCoordinator() as {
      initializeFromDefinitions: (
        manager: VariableManager,
        variables: VariableDefinition[]
      ) => void;
    };
    
    // Initialize variables from WorkflowGraph definitions
    variableCoordinator.initializeFromDefinitions(
      workflowExecutionEntity.variableStateManager,
      workflowGraph.variables || []
    );

    // Step 7: Create MessageContextRegistry and initialize contexts
    const messageContextRegistry = new InMemoryMessageContextRegistry();
    
    // Initialize execution contexts based on workflow config
    let workflowConfig: WorkflowConfig | undefined;
    if (workflowId) {
      try {
        const workflowRegistry = this.getWorkflowRegistry();
        const workflowTemplate = workflowRegistry.get(workflowId);
        workflowConfig = workflowTemplate?.config;
      } catch (error) {
        logger.warn(`Failed to get workflow config for initialization`, { workflowId, error });
      }
    }
    initializeExecutionContext(messageContextRegistry, workflowConfig);
    
    // Attach registry to workflow execution for handlers to access
    (workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }).messageContextRegistry = messageContextRegistry;

    // Step 7.5: Initialize ToolPermissionManager if AvailableTools is configured
    if (workflowConfig?.availableTools) {
      try {
        const availableToolsConfig = workflowConfig.availableTools as AvailableTools;
        const schemaTools = resolveSchemaTools(availableToolsConfig);
        const initialTools = resolveInitialTools(availableToolsConfig);
        
        const permissionManager = new ToolPermissionManager(initialTools, schemaTools);
        
        // Store in DI container for this execution
        // Note: This adds a new binding that will take precedence over the placeholder
        if (this.globalContext) {
          this.globalContext.container.bind(Identifiers.ToolPermissionManager).toConstantValue(permissionManager);
        }
        
        logger.info('ToolPermissionManager initialized for workflow', {
          workflowId,
          executionId,
          schemaToolsCount: schemaTools.length,
          initialToolsCount: initialTools.length,
        });
      } catch (error) {
        logger.warn('Failed to initialize ToolPermissionManager', { workflowId, executionId, error });
      }
    }

    // Step 8: Create ConversationSession
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      executionId: workflowExecution.id,
      workflowId: workflowGraph.workflowId,
    });

    // Step 9: Create WorkflowStateCoordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity,
      conversationManager,
    });

    return {
      workflowExecutionEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Build WorkflowExecutionEntity from cached template
   *
   * @param templateId Template ID
   * @returns WorkflowExecutionBuildResult containing WorkflowExecutionEntity, WorkflowStateCoordinator, and ConversationSession
   */
  async buildFromTemplate(templateId: string): Promise<WorkflowExecutionBuildResult> {
    const template = this.workflowExecutionTemplates.get(templateId);
    if (!template) {
      throw new RuntimeValidationError(`Workflow execution template not found: ${templateId}`, {
        field: "templateId",
        value: templateId,
      });
    }

    // Deep copy template
    return await this.createCopy(template);
  }

  /**
   * Create a copy of WorkflowExecutionEntity
   *
   * @param sourceWorkflowExecutionEntity Source WorkflowExecutionEntity
   * @returns WorkflowExecutionBuildResult containing WorkflowExecutionEntity, WorkflowStateCoordinator, and ConversationSession
   */
  async createCopy(
    sourceWorkflowExecutionEntity: WorkflowExecutionEntity,
  ): Promise<WorkflowExecutionBuildResult> {
    const sourceWorkflowExecution = sourceWorkflowExecutionEntity.getWorkflowExecutionData();
    const copiedExecutionId = generateId();

    logger.info("Creating workflow execution copy", {
      sourceExecutionId: sourceWorkflowExecution.id,
      copiedExecutionId,
    });

    const copiedWorkflowExecution: WorkflowExecution = {
      id: copiedExecutionId,
      workflowId: sourceWorkflowExecution.workflowId,
      workflowVersion: sourceWorkflowExecution.workflowVersion,
      currentNodeId: sourceWorkflowExecution.currentNodeId,
      graph: sourceWorkflowExecution.graph,
      variables: sourceWorkflowExecution.variables.map(v => ({ ...v })),
      input: { ...sourceWorkflowExecution.input },
      output: { ...sourceWorkflowExecution.output },
      nodeResults: sourceWorkflowExecution.nodeResults.map(h => ({ ...h })),
      errors: [],
      executionType: "TRIGGERED_SUBWORKFLOW",
      triggeredSubworkflowContext: {
        parentExecutionId: sourceWorkflowExecution.id,
        childExecutionIds: [],
        triggeredSubworkflowId: "",
      },
    };

    // Create WorkflowExecutionState
    const workflowExecutionState = new WorkflowExecutionState();

    // Create ExecutionState (for subgraph stack management)
    const executionState = new ExecutionState();

    // Create WorkflowExecutionEntity
    // Note: Each entity has its own VariableManager instance
    const registry = this.getExecutionHierarchyRegistry();
    const copiedWorkflowExecutionEntity = new WorkflowExecutionEntity(
      copiedWorkflowExecution,
      executionState,
      workflowExecutionState,
      undefined,
      registry
    );

    // Copy variable state from source entity to copied entity
    copiedWorkflowExecutionEntity.variableStateManager.copyFrom(
      sourceWorkflowExecutionEntity.variableStateManager
    );

    // Create ConversationSession (clone from source message history)
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      workflowExecutionId: copiedWorkflowExecution.id,
      workflowId: copiedWorkflowExecution.workflowId,
      initialMessages: sourceWorkflowExecutionEntity.messageHistoryManager.getMessages(),
    });
    conversationManager.setContext(copiedWorkflowExecution.workflowId, copiedWorkflowExecution.id);

    // Create WorkflowStateCoordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity: copiedWorkflowExecutionEntity,
      conversationManager,
    });

    logger.debug("Workflow execution copy created", {
      copiedExecutionId,
      sourceExecutionId: sourceWorkflowExecution.id,
    });

    return {
      workflowExecutionEntity: copiedWorkflowExecutionEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Unified child execution creation method
   * 
   * Replaces createSubgraph(), createFork(), and triggered workflow build() with a single API.
   * All child execution types share the same creation logic but differ in variable initialization strategies.
   * 
   * @param parent Parent workflow execution entity
   * @param options Child execution type and configuration
   * @returns WorkflowExecutionBuildResult containing the child entity, state coordinator, and conversation manager
   */
  async createChildExecution(
    parent: WorkflowExecutionEntity,
    options: ChildExecutionOptions
  ): Promise<WorkflowExecutionBuildResult> {
    const { type, config } = options;
    
    logger.info("Creating child execution", {
      parentExecutionId: parent.id,
      childType: type,
      subworkflowId: config.subworkflowId,
      forkPathId: config.forkPathId,
    });

    // Step 1: Validate configuration
    this.validateChildExecutionConfig(type, config);
    
    // Step 2: Get target workflow graph
    const targetGraph = await this.getTargetGraph(config);
    
    // Step 3: Create execution entity (shared logic for all types)
    const executionId = generateId();
    const childEntity = this.createExecutionEntity(
      executionId,
      targetGraph,
      type,
      config,
      parent
    );
    
    // Step 4: Initialize variables (different strategies per type)
    await this.initializeVariables(childEntity, parent, type, config);
    
    // Step 5: Establish hierarchy relationship (unified using ExecutionHierarchyRegistry)
    this.establishHierarchy(parent, childEntity, type, config);
    
    // Step 6: Create conversation session
    const conversationManager = this.createConversationSession(
      childEntity,
      parent
    );
    
    // Step 7: Create state coordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity: childEntity,
      conversationManager,
    });
    
    logger.debug("Child execution created successfully", {
      childExecutionId: childEntity.id,
      parentExecutionId: parent.id,
      childType: type,
    });
    
    return {
      workflowExecutionEntity: childEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Validate child execution configuration
   */
  private validateChildExecutionConfig(
    type: ChildExecutionType,
    config: ChildExecutionConfig
  ): void {
    switch (type) {
      case 'SUBGRAPH':
        if (!config.subworkflowId) {
          throw new RuntimeValidationError('SUBGRAPH requires subworkflowId', {
            field: 'subworkflowId',
          });
        }
        if (!config.nodeId) {
          throw new RuntimeValidationError('SUBGRAPH requires nodeId', {
            field: 'nodeId',
          });
        }
        break;
        
      case 'FORK_BRANCH':
        if (!config.forkPathId) {
          throw new RuntimeValidationError('FORK_BRANCH requires forkPathId', {
            field: 'forkPathId',
          });
        }
        if (!config.startNodeId) {
          throw new RuntimeValidationError('FORK_BRANCH requires startNodeId', {
            field: 'startNodeId',
          });
        }
        break;
        
      case 'TRIGGERED':
        if (!config.subworkflowId) {
          throw new RuntimeValidationError('TRIGGERED requires subworkflowId', {
            field: 'subworkflowId',
          });
        }
        break;
    }
  }

  /**
   * Get target workflow graph based on configuration
   */
  private async getTargetGraph(config: ChildExecutionConfig): Promise<WorkflowGraph> {
    const subworkflowId = config.subworkflowId;
    
    if (!subworkflowId) {
      // FORK_BRANCH uses parent's graph
      throw new RuntimeValidationError('Cannot determine target graph', {
        field: 'subworkflowId',
      });
    }
    
    const graph = this.getWorkflowGraphRegistry().get(subworkflowId);
    if (!graph) {
      throw new ExecutionError(
        `Workflow '${subworkflowId}' not found or not preprocessed`,
        undefined,
        subworkflowId
      );
    }
    
    return graph;
  }

  /**
   * Create execution entity (shared logic for all child types)
   */
  private createExecutionEntity(
    executionId: string,
    graph: WorkflowGraph,
    type: ChildExecutionType,
    config: ChildExecutionConfig,
    parent: WorkflowExecutionEntity
  ): WorkflowExecutionEntity {
    
    // Determine start node
    let startNodeId: string;
    if (type === 'FORK_BRANCH' && config.startNodeId) {
      startNodeId = config.startNodeId;
    } else {
      const startNode = Array.from(graph.nodes.values()).find(n => n.type === "START");
      if (!startNode) {
        throw new RuntimeValidationError("Workflow graph must have a START node", {
          field: "graph.nodes",
        });
      }
      startNodeId = startNode.id;
    }
    
    // Determine execution type
    const executionTypeMap: Record<ChildExecutionType, string> = {
      'SUBGRAPH': 'SUBGRAPH',
      'FORK_BRANCH': 'FORK_JOIN',
      'TRIGGERED': 'TRIGGERED_SUBWORKFLOW',
    };
    
    // Create workflow execution data
    const execution: WorkflowExecution = {
      id: executionId,
      workflowId: graph.workflowId,
      workflowVersion: graph.workflowVersion,
      currentNodeId: startNodeId,
      graph,
      variables: [],
      input: {},
      output: {},
      nodeResults: [],
      errors: [],
      executionType: executionTypeMap[type] as any,
    };
    if (type === 'SUBGRAPH') {
      execution.hierarchy = {
        parent: {
          parentType: 'WORKFLOW',
          parentId: parent.id,
          nodeId: config.nodeId,
        },
        children: [],
        depth: parent.getHierarchyMetadata()?.depth ? 
               parent.getHierarchyMetadata()!.depth + 1 : 1,
        rootExecutionId: parent.getRootExecutionId(),
        rootExecutionType: parent.getRootExecutionType(),
      };
    } else if (type === 'FORK_BRANCH') {
      execution.forkJoinContext = {
        forkId: config.forkPathId || '',
        forkPathId: config.forkPathId || '',
      };
    }
    
    // Create states
    const executionState = new ExecutionState();
    const workflowExecutionState = new WorkflowExecutionState();
    const registry = this.getExecutionHierarchyRegistry();
    
    return new WorkflowExecutionEntity(
      execution,
      executionState,
      workflowExecutionState,
      undefined,
      registry
    );
  }

  /**
   * Initialize variables based on child execution type
   */
  private async initializeVariables(
    child: WorkflowExecutionEntity,
    parent: WorkflowExecutionEntity,
    type: ChildExecutionType,
    config: ChildExecutionConfig
  ): Promise<void> {
    switch (type) {
      case 'SUBGRAPH':
        // Subgraph: explicit mapping + deep clone
        if (config.variableMapping?.inputs && config.variableMapping.inputs.length > 0) {
          child.variableStateManager.importVariables(
            parent.variableStateManager,
            config.variableMapping.inputs
          );
          logger.debug("Imported variables to subgraph", {
            count: config.variableMapping.inputs.length,
          });
        }

        // Subgraph: process data mapping from parent execution input to child variables
        if (config.dataMapping?.inputs && config.dataMapping.inputs.length > 0) {
          const parentInput = parent.getInput();
          for (const mapping of config.dataMapping.inputs) {
            const value = parentInput[mapping.parentField];
            if (value !== undefined) {
              child.variableStateManager.setVariable(mapping.childVariableName, value);
            } else if (mapping.required) {
              throw new RuntimeValidationError(
                `Required data input '${mapping.parentField}' not found in parent workflow input`,
                {
                  operation: "initializeVariables",
                  field: mapping.parentField,
                }
              );
            } else if (mapping.defaultValue !== undefined) {
              child.variableStateManager.setVariable(mapping.childVariableName, mapping.defaultValue);
            }
          }
          logger.debug("Mapped data inputs to subgraph variables", {
            count: config.dataMapping.inputs.length,
          });
        }
        break;
        
      case 'FORK_BRANCH':
        // Fork: complete deep clone
        child.variableStateManager.copyFrom(parent.variableStateManager);
        logger.debug("Copied variables to fork branch (deep clone)");
        break;
        
      case 'TRIGGERED':
        // Triggered: will be handled separately via input mapping
        // Variables will be set through prepareInputData in triggered-subworkflow-handler
        logger.debug("Triggered workflow - variables will be set via input mapping");
        break;
    }
    
    // All types: initialize variables from workflow definitions
    const variableCoordinator = this.getVariableCoordinator() as {
      initializeFromDefinitions: (
        manager: any,
        variables: VariableDefinition[]
      ) => void;
    };
    
    variableCoordinator.initializeFromDefinitions(
      child.variableStateManager,
      child.getWorkflowExecutionData().graph.variables || []
    );
  }

  /**
   * Establish parent-child hierarchy relationship WITH interruption cascade propagation
   */
  private establishHierarchy(
    parent: WorkflowExecutionEntity,
    child: WorkflowExecutionEntity,
    type: ChildExecutionType,
    config: ChildExecutionConfig
  ): void {
    const registry = this.getExecutionHierarchyRegistry();
    
    // Step 1: Register to global registry
    registry.register(child);
    
    // Step 2: Set parent context
    child.setParentContext({
      parentType: 'WORKFLOW',
      parentId: parent.id,
      ...(config.nodeId && { nodeId: config.nodeId }),
    });
    
    // Step 3: Register child reference in parent entity
    const shouldInheritInterruption = this.shouldInheritInterruption(type, config);
    parent.registerChild({
      childType: 'WORKFLOW',
      childId: child.id,
      createdAt: Date.now(),
      ...(config.forkPathId && { forkPathId: config.forkPathId }),
      inheritsInterruption: shouldInheritInterruption,
    });
    
    // Step 4: Setup interruption cascade propagation (NEW)
    if (shouldInheritInterruption) {
      const parentInterruptionState = parent.getInterruptionState();
      const childInterruptionState = child.getInterruptionState();
      
      if (parentInterruptionState && childInterruptionState) {
        // Register child with parent's interruption state
        parentInterruptionState.registerChild(childInterruptionState);
        
        logger.info("Interruption cascade established", {
          parentExecutionId: parent.id,
          childExecutionId: child.id,
          childType: type,
        });
      }
    }
    
    logger.debug('Child hierarchy established', {
      childExecutionId: child.id,
      parentExecutionId: parent.id,
      childType: type,
      forkPathId: config.forkPathId,
      inheritsInterruption: shouldInheritInterruption,
    });
  }

  /**
   * Create conversation session for child execution
   */
  private createConversationSession(
    child: WorkflowExecutionEntity,
    parent: WorkflowExecutionEntity
  ): ConversationSession {
    const childExecution = child.getWorkflowExecutionData();
    
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      workflowExecutionId: childExecution.id,
      workflowId: childExecution.workflowId,
      initialMessages: parent.messageHistoryManager.getMessages(),
    });
    conversationManager.setContext(childExecution.workflowId, childExecution.id);
    
    return conversationManager;
  }
  
  /**
   * Determine whether child should inherit parent's interruption state
   */
  private shouldInheritInterruption(
    type: ChildExecutionType,
    config: ChildExecutionConfig
  ): boolean {
    switch (type) {
      case 'SUBGRAPH':
        return true; // Synchronous, always inherits
        
      case 'FORK_BRANCH':
        return true; // Synchronous, always inherits
        
      case 'TRIGGERED':
        // Explicitly check execution mode for clarity
        // Default to false (asynchronous, independent) if not specified
        return config.async === true ? false : true;
        
      default:
        return false;
    }
  }

  /**
   * Clear cache
   */
  clearCache(): void {
    this.workflowExecutionTemplates.clear();
  }

  /**
   * Invalidate cache for specified workflow
   *
   * @param workflowId Workflow ID
   */
  invalidateWorkflow(workflowId: string): void {
    // Workflow execution templates related to workflow
    for (const [templateId, template] of this.workflowExecutionTemplates.entries()) {
      if (template.getWorkflowId() === workflowId) {
        this.workflowExecutionTemplates.delete(templateId);
      }
    }
  }
}
