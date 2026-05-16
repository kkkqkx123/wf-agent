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
      variableScopes: {
        global: {},
        execution: {},
      },
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
      // 2-level scopes: global is shared through references, execution creates deep copies
      variableScopes: {
        global: sourceWorkflowExecution.variableScopes.global,
        execution: { ...sourceWorkflowExecution.variableScopes.execution },
      },
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
   * Create a fork sub-WorkflowExecutionEntity
   *
   * @param parentWorkflowExecutionEntity Parent WorkflowExecutionEntity
   * @param forkConfig Fork configuration
   * @returns WorkflowExecutionBuildResult containing WorkflowExecutionEntity, WorkflowStateCoordinator, and ConversationSession
   */
  async createFork(
    parentWorkflowExecutionEntity: WorkflowExecutionEntity,
    forkConfig: { forkId: string; forkPathId?: string; startNodeId?: string },
  ): Promise<WorkflowExecutionBuildResult> {
    const parentWorkflowExecution = parentWorkflowExecutionEntity.getWorkflowExecutionData();
    const forkExecutionId = generateId();

    logger.info("Creating fork workflow execution", {
      parentExecutionId: parentWorkflowExecution.id,
      forkExecutionId,
      forkId: forkConfig.forkId,
      forkPathId: forkConfig.forkPathId,
    });

    // Separate workflow execution variables from global variables
    const workflowExecutionVariables = [];

    for (const variable of parentWorkflowExecution.variables) {
      if (variable.scope === "execution") {
        workflowExecutionVariables.push({ ...variable });
      }
      // Global variables are not copied to child workflow executions; instead, they are shared through references
    }

    const forkWorkflowExecution: WorkflowExecution = {
      id: forkExecutionId,
      workflowId: parentWorkflowExecution.workflowId,
      workflowVersion: parentWorkflowExecution.workflowVersion,
      currentNodeId: forkConfig.startNodeId || parentWorkflowExecution.currentNodeId,
      graph: parentWorkflowExecution.graph,
      variables: workflowExecutionVariables,
      // 2-level scopes: global is shared via references, execution creates deep copies
      variableScopes: {
        global: parentWorkflowExecution.variableScopes.global,
        execution: { ...parentWorkflowExecution.variableScopes.execution },
      },
      input: { ...parentWorkflowExecution.input },
      output: {},
      nodeResults: [],
      errors: [],
      executionType: "FORK_JOIN",
      forkJoinContext: {
        forkId: forkConfig.forkId,
        forkPathId: forkConfig.forkPathId || forkConfig.forkId,
      },
    };

    // Create WorkflowExecutionState
    const workflowExecutionState = new WorkflowExecutionState();

    // Create ExecutionState (for subgraph stack management)
    const executionState = new ExecutionState();

    // Create WorkflowExecutionEntity
    // Note: Each fork execution has its own VariableManager instance
    const registry = this.getExecutionHierarchyRegistry();
    const forkWorkflowExecutionEntity = new WorkflowExecutionEntity(
      forkWorkflowExecution,
      executionState,
      workflowExecutionState,
      undefined,
      registry
    );

    // Copy variable state from parent to fork using deep clone
    // This ensures complete isolation between fork branches
    forkWorkflowExecutionEntity.variableStateManager.copyFrom(
      parentWorkflowExecutionEntity.variableStateManager
    );
    
    // TODO Phase 2 Enhancement: Support explicit variable mapping for forks
    // If fork branch has custom variableMapping config, use importVariables instead:
    // if (forkBranchConfig.variableMapping) {
    //   forkWorkflowExecutionEntity.variableStateManager.importVariables(
    //     parentWorkflowExecutionEntity.variableStateManager,
    //     forkBranchConfig.variableMapping
    //   );
    // }

    // Create ConversationSession (clone from parent message history)
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      workflowExecutionId: forkWorkflowExecution.id,
      workflowId: forkWorkflowExecution.workflowId,
      initialMessages: parentWorkflowExecutionEntity.messageHistoryManager.getMessages(),
    });
    conversationManager.setContext(forkWorkflowExecution.workflowId, forkWorkflowExecution.id);

    // Create WorkflowStateCoordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity: forkWorkflowExecutionEntity,
      conversationManager,
    });

    logger.debug("Fork workflow execution created", {
      forkExecutionId,
      parentExecutionId: parentWorkflowExecution.id,
    });

    return {
      workflowExecutionEntity: forkWorkflowExecutionEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Create a subgraph execution entity (Phase 1: Scheme C - Separate Execution Entity)
   * 
   * Creates an independent child workflow execution with explicit variable mapping and complete isolation.
   * This replaces the old graph expansion model with a proper parent-child execution relationship.
   *
   * @param parentEntity Parent workflow execution entity
   * @param options Subgraph creation options including subworkflow ID and variable mappings
   * @returns WorkflowExecutionBuildResult containing the child entity, state coordinator, and conversation manager
   */
  async createSubgraph(
    parentEntity: WorkflowExecutionEntity,
    options: {
      subworkflowId: string;
      nodeId: string; // SUBGRAPH node ID in parent workflow
      variableMapping?: {
        inputs?: Array<{ externalName: string; internalName: string; required?: boolean; defaultValue?: unknown }>;
        outputs?: Array<{ internalName: string; externalName: string }>;
      };
      async?: boolean;
    }
  ): Promise<WorkflowExecutionBuildResult> {
    const parentExecution = parentEntity.getWorkflowExecutionData();
    const subgraphExecutionId = generateId();
    
    logger.info("Creating subgraph execution", {
      parentExecutionId: parentEntity.id,
      subgraphExecutionId,
      subworkflowId: options.subworkflowId,
    });

    // Step 1: Get the preprocessed subworkflow graph
    const subgraphGraph = this.getWorkflowGraphRegistry().get(options.subworkflowId);
    if (!subgraphGraph) {
      throw new ExecutionError(
        `Subworkflow '${options.subworkflowId}' not found or not preprocessed`,
        undefined,
        options.subworkflowId
      );
    }

    // Step 2: Create subworkflow execution data
    const startNode = Array.from(subgraphGraph.nodes.values()).find(n => n.type === "START");
    if (!startNode) {
      throw new RuntimeValidationError("Subworkflow graph must have a START node", {
        field: "graph.nodes",
      });
    }

    const subgraphExecution: WorkflowExecution = {
      id: subgraphExecutionId,
      workflowId: options.subworkflowId,
      workflowVersion: subgraphGraph.workflowVersion,
      currentNodeId: startNode.id,
      graph: subgraphGraph,
      variables: [], // Initially empty, will be populated via importVariables
      variableScopes: {
        global: parentExecution.variableScopes.global, // Share global scope by reference
        execution: {}, // Empty execution scope - will be filled via importVariables
      },
      input: {}, // Will be populated via variable inputs
      output: {},
      nodeResults: [],
      errors: [],
      executionType: "TRIGGERED_SUBWORKFLOW", // Use existing type for subgraph executions
      hierarchy: {
        parent: {
          parentType: 'WORKFLOW',
          parentId: parentEntity.id,
          nodeId: options.nodeId, // SUBGRAPH node ID in parent
        },
        children: [],
        depth: parentEntity.getHierarchyMetadata()?.depth ? 
               parentEntity.getHierarchyMetadata()!.depth + 1 : 1,
        rootExecutionId: parentEntity.getRootExecutionId(),
        rootExecutionType: parentEntity.getRootExecutionType(),
      },
    };

    // Step 3: Create ExecutionState and WorkflowExecutionState
    const executionState = new ExecutionState();
    const workflowExecutionState = new WorkflowExecutionState();
    const registry = this.getExecutionHierarchyRegistry();

    // Step 4: Create WorkflowExecutionEntity
    const subgraphEntity = new WorkflowExecutionEntity(
      subgraphExecution,
      executionState,
      workflowExecutionState,
      undefined,
      registry
    );

    // Step 5: Import variables from parent using explicit mapping (with deep clone)
    if (options.variableMapping?.inputs && options.variableMapping.inputs.length > 0) {
      subgraphEntity.variableStateManager.importVariables(
        parentEntity.variableStateManager,
        options.variableMapping.inputs
      );
      logger.debug("Imported variables to subgraph", {
        count: options.variableMapping.inputs.length,
      });
    }

    // Step 6: Initialize variables from subworkflow definitions
    const variableCoordinator = this.getVariableCoordinator() as {
      initializeFromDefinitions: (
        manager: any,
        variables: VariableDefinition[]
      ) => void;
    };
    variableCoordinator.initializeFromDefinitions(
      subgraphEntity.variableStateManager,
      subgraphGraph.variables || []
    );

    // Step 7: Create ConversationSession (clone from parent message history)
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      workflowExecutionId: subgraphExecutionId,
      workflowId: options.subworkflowId,
      initialMessages: parentEntity.messageHistoryManager.getMessages(),
    });
    conversationManager.setContext(subgraphExecution.workflowId, subgraphExecutionId);

    // Step 8: Create WorkflowStateCoordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity: subgraphEntity,
      conversationManager,
    });

    // Step 9: Register parent-child relationship in hierarchy registry
    registry.register(subgraphEntity);
    parentEntity.registerChild({
      childType: 'WORKFLOW',
      childId: subgraphExecutionId,
      createdAt: Date.now(),
    });

    logger.debug("Subgraph execution created successfully", {
      subgraphExecutionId,
      parentExecutionId: parentEntity.id,
      variableInputCount: options.variableMapping?.inputs?.length || 0,
      hasOutputs: !!(options.variableMapping?.outputs && options.variableMapping.outputs.length > 0),
    });

    return {
      workflowExecutionEntity: subgraphEntity,
      stateCoordinator,
      conversationManager,
    };
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
