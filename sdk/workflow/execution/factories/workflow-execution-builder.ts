/**
 * WorkflowExecutionBuilder - Workflow Execution Builder
 *
 * Responsible for retrieving WorkflowDefinition from WorkflowRegistry and creating WorkflowExecutionEntity instances.
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

import type { WorkflowGraph } from "@wf-agent/types";
import type { WorkflowExecution, WorkflowExecutionOptions, WorkflowExecutionStatus } from "@wf-agent/types";
import { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import { WorkflowExecutionState } from "../../state-managers/workflow-execution-state.js";
import { ExecutionState } from "../../state-managers/execution-state.js";
import { WorkflowStateCoordinator } from "../../state-managers/workflow-state-coordinator.js";
import { generateId, now as getCurrentTimestamp } from "@wf-agent/common-utils";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";
import type { WorkflowGraphRegistry } from "../../stores/workflow-graph-registry.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import { ConversationSession } from "../../../core/messaging/conversation-session.js";
import { logError, emitErrorEvent } from "../../../core/utils/error-utils.js";

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

  constructor() {}

  /**
   * Get workflow graph registry (from DI container)
   */
  private getWorkflowGraphRegistry(): WorkflowGraphRegistry {
    const container = getContainer();
    return container.get(Identifiers.WorkflowRegistry) as WorkflowGraphRegistry;
  }

  /**
   * Get variable coordinator (from DI container)
   */
  private getVariableCoordinator(): unknown {
    const container = getContainer();
    return container.get(Identifiers.VariableCoordinator);
  }

  /**
   * Get variable state manager (from DI container)
   */
  private getVariableStateManager(): unknown {
    const container = getContainer();
    return container.get(Identifiers.VariableState);
  }

  /**
   * Get event manager (from DI container)
   */
  private getEventManager(): EventRegistry {
    const container = getContainer();
    return container.get(Identifiers.EventRegistry) as EventRegistry;
  }

  /**
   * Get tool service (from DI container)
   */
  private getToolService(): ToolRegistry {
    const container = getContainer();
    return container.get(Identifiers.ToolRegistry) as ToolRegistry;
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
    const result = await this.buildFromWorkflowGraph(workflowGraph, options);

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
    const now = getCurrentTimestamp();

    const workflowExecution: WorkflowExecution = {
      id: executionId,
      workflowId: workflowGraph.workflowId,
      workflowVersion: workflowGraph.workflowVersion,
      currentNodeId: startNode.id,
      graph: workflowExecutionGraphData,
      variables: [],
      variableScopes: {
        global: {},
        thread: {},
        workflowExecution: {},
        local: [],
        loop: [],
      },
      input: options.input || {},
      output: {},
      nodeResults: [],
      errors: [],
      executionType: "MAIN",
    };

    // Step 4: Initialize variables from WorkflowGraph
    const variableCoordinator = this.getVariableCoordinator() as {
      initializeFromWorkflow: (variables: unknown[]) => void;
    };
    variableCoordinator.initializeFromWorkflow(workflowGraph.variables || []);

    // Step 5: Create WorkflowExecutionState
    const workflowExecutionState = new WorkflowExecutionState();

    // Step 5.1: Create ExecutionState (for subgraph stack management)
    const executionState = new ExecutionState();

    // Step 6: Create WorkflowExecutionEntity (without ConversationManager)
    const workflowExecutionEntity = new WorkflowExecutionEntity(
      workflowExecution,
      executionState,
      workflowExecutionState,
    );

    // Step 7: Create ConversationSession
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      executionId: workflowExecution.id,
      workflowId: workflowGraph.workflowId,
    });

    // Step 8: Create WorkflowStateCoordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity,
      conversationManager,
    });

    return {
      workflowExecutionEntity,
      stateCoordinator,
      conversationManager,
      executionEntity: workflowExecutionEntity, // Backward compatibility
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

    const now = getCurrentTimestamp();

    const copiedWorkflowExecution: WorkflowExecution = {
      id: copiedExecutionId,
      workflowId: sourceWorkflowExecution.workflowId,
      workflowVersion: sourceWorkflowExecution.workflowVersion,
      currentNodeId: sourceWorkflowExecution.currentNodeId,
      graph: sourceWorkflowExecution.graph,
      variables: sourceWorkflowExecution.variables.map(v => ({ ...v })),
      // 4-level scopes: global is shared through references, thread creates deep copies, local and loop values are cleared
      variableScopes: {
        global: sourceWorkflowExecution.variableScopes.global,
        thread: { ...sourceWorkflowExecution.variableScopes.thread },
        workflowExecution: { ...sourceWorkflowExecution.variableScopes.workflowExecution },
        local: [],
        loop: [],
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

    // Create WorkflowExecutionEntity (without ConversationManager)
    const copiedWorkflowExecutionEntity = new WorkflowExecutionEntity(
      copiedWorkflowExecution,
      executionState,
      workflowExecutionState,
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
      executionEntity: copiedWorkflowExecutionEntity,
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

    const now = getCurrentTimestamp();

    // Separate workflow execution variables from global variables
    const workflowExecutionVariables = [];

    for (const variable of parentWorkflowExecution.variables) {
      if (variable.scope === "workflowExecution") {
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
      // 4-level scopes: global is shared via references, workflowExecution creates deep copies, while local and loop scopes clear their contents upon exit
      variableScopes: {
        global: parentWorkflowExecution.variableScopes.global,
        thread: { ...parentWorkflowExecution.variableScopes.thread },
        workflowExecution: { ...parentWorkflowExecution.variableScopes.workflowExecution },
        local: [],
        loop: [],
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

    // Create WorkflowExecutionEntity (without ConversationManager)
    const forkWorkflowExecutionEntity = new WorkflowExecutionEntity(
      forkWorkflowExecution,
      executionState,
      workflowExecutionState,
    );

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
      executionEntity: forkWorkflowExecutionEntity,
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
