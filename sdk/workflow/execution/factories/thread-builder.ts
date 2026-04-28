/**
 * ThreadBuilder - Thread Builder
 *
 * Responsible for retrieving WorkflowDefinition from WorkflowRegistry and creating WorkflowExecutionEntity instances.
 * Provides support for thread template caching and deep copying, as well as the use of preprocessed graphs and graph navigation.
 *
 * Note: ThreadBuilder is used directly for thread creation, not wrapped as ThreadFactory.
 * The naming difference from AgentLoopFactory reflects the different execution models.
 *
 * Migration Note:
 * - WorkflowExecutionEntity no longer holds ConversationManager directly
 * - ThreadStateCoordinator is created separately to manage state
 * - This eliminates data redundancy and synchronization issues
 */

import type { PreprocessedGraph } from "@wf-agent/types";
import type { Thread, ThreadOptions, WorkflowExecutionStatus } from "@wf-agent/types";
import { WorkflowExecutionEntity } from "../../entities/index.js";
import { ExecutionState } from "../../state-managers/execution-state.js";
import { ThreadStateCoordinator } from "../../state-managers/thread-state-coordinator.js";
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

const logger = createContextualLogger({ operation: "thread-builder" });

/**
 * Thread Build Result
 * Contains both WorkflowExecutionEntity and ThreadStateCoordinator
 */
export interface ThreadBuildResult {
  threadEntity: WorkflowExecutionEntity;
  stateCoordinator: WorkflowStateCoordinator;
  conversationManager: ConversationSession;
}

/**
 * ThreadBuilder - Thread Builder
 *
 * Factory class for creating WorkflowExecutionEntity instances from workflows.
 */
export class ThreadBuilder {
  private threadTemplates: Map<string, WorkflowExecutionEntity> = new Map();

  constructor() {}

  /**
   * Get graph registry (from DI container)
   */
  private getGraphRegistry(): WorkflowGraphRegistry {
    const container = getContainer();
    return container.get(Identifiers.WorkflowGraphRegistry) as WorkflowGraphRegistry;
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
   * @param options Thread options
   * @returns ThreadBuildResult containing WorkflowExecutionEntity, ThreadStateCoordinator, and ConversationSession
   */
  async build(workflowId: string, options: ThreadOptions = {}): Promise<ThreadBuildResult> {
    logger.info("Building thread from workflow", { workflowId });

    // Get preprocessed graph from graph-registry
    const preprocessedGraph = this.getGraphRegistry().get(workflowId);

    if (!preprocessedGraph) {
      const error = new ExecutionError(
        `Workflow '${workflowId}' not found or not preprocessed`,
        undefined,
        workflowId,
      );
      const eventManager = this.getEventManager();
      logError(error, { workflowId });
      emitErrorEvent(eventManager, {
        threadId: "",
        workflowId,
        error,
      });
      throw error;
    }

    // Build from PreprocessedGraph
    const result = await this.buildFromPreprocessedGraph(preprocessedGraph, options);

    logger.info("Thread built successfully", { threadId: result.workflowExecutionEntity.id, workflowId });

    return result;
  }

  /**
   * Build WorkflowExecutionEntity from PreprocessedGraph (internal method)
   *
   * @param preprocessedGraph Preprocessed graph
   * @param options Thread options
   * @returns ThreadBuildResult containing WorkflowExecutionEntity, ThreadStateCoordinator, and ConversationSession
   */
  private async buildFromPreprocessedGraph(
    preprocessedGraph: PreprocessedGraph,
    options: ThreadOptions = {},
  ): Promise<ThreadBuildResult> {
    // Step 1: Verify preprocessed graph
    if (!preprocessedGraph.nodes || preprocessedGraph.nodes.size === 0) {
      throw new RuntimeValidationError("Preprocessed graph must have at least one node", {
        field: "graph.nodes",
      });
    }

    const startNode = Array.from(preprocessedGraph.nodes.values()).find(n => n.type === "START");
    if (!startNode) {
      throw new RuntimeValidationError("Preprocessed graph must have a START node", {
        field: "graph.nodes",
      });
    }

    const endNode = Array.from(preprocessedGraph.nodes.values()).find(n => n.type === "END");
    if (!endNode) {
      throw new RuntimeValidationError("Preprocessed graph must have an END node", {
        field: "graph.nodes",
      });
    }

    // Step 2: PreprocessedGraph itself is a Graph containing complete graph structure
    const threadGraphData = preprocessedGraph;

    // Step 3: Create Thread instance
    const threadId = generateId();
    const now = getCurrentTimestamp();

    const thread: Thread = {
      id: threadId,
      workflowId: preprocessedGraph.workflowId,
      workflowVersion: preprocessedGraph.workflowVersion,
      currentNodeId: startNode.id,
      graph: threadGraphData,
      variables: [],
      variableScopes: {
        global: {},
        thread: {},
        local: [],
        loop: [],
      },
      input: options.input || {},
      output: {},
      nodeResults: [],
      errors: [],
      threadType: "MAIN",
    };

    // Step 4: Initialize variables from PreprocessedGraph
    const variableCoordinator = this.getVariableCoordinator() as {
      initializeFromWorkflow: (variables: unknown[]) => void;
    };
    variableCoordinator.initializeFromWorkflow(preprocessedGraph.variables || []);

    // Step 5: Create ExecutionState
    const executionState = new ExecutionState();

    // Step 6: Create WorkflowExecutionEntity (without ConversationManager)
    const threadEntity = new WorkflowExecutionEntity(thread, executionState);

    // Step 7: Create ConversationSession
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      threadId: thread.id,
      workflowId: preprocessedGraph.workflowId,
    });

    // Step 8: Create ThreadStateCoordinator
    const stateCoordinator = new ThreadStateCoordinator({
      threadEntity,
      conversationManager,
    });

    return {
      threadEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Build WorkflowExecutionEntity from cached template
   *
   * @param templateId Template ID
   * @returns ThreadBuildResult containing WorkflowExecutionEntity, ThreadStateCoordinator, and ConversationSession
   */
  async buildFromTemplate(templateId: string): Promise<ThreadBuildResult> {
    const template = this.threadTemplates.get(templateId);
    if (!template) {
      throw new RuntimeValidationError(`Thread template not found: ${templateId}`, {
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
   * @param sourceThreadEntity Source WorkflowExecutionEntity
   * @returns ThreadBuildResult containing WorkflowExecutionEntity, ThreadStateCoordinator, and ConversationSession
   */
  async createCopy(sourceThreadEntity: WorkflowExecutionEntity): Promise<ThreadBuildResult> {
    const sourceThread = sourceThreadEntity.getThread();
    const copiedThreadId = generateId();

    logger.info("Creating thread copy", {
      sourceThreadId: sourceThread.id,
      copiedThreadId,
    });

    const now = getCurrentTimestamp();

    const copiedThread: Thread = {
      id: copiedThreadId,
      workflowId: sourceThread.workflowId,
      workflowVersion: sourceThread.workflowVersion,
      currentNodeId: sourceThread.currentNodeId,
      graph: sourceThread.graph,
      variables: sourceThread.variables.map(v => ({ ...v })),
      // 4-level scopes: global is shared through references, thread creates deep copies, local and loop values are cleared
      variableScopes: {
        global: sourceThread.variableScopes.global,
        thread: { ...sourceThread.variableScopes.thread },
        local: [],
        loop: [],
      },
      input: { ...sourceThread.input },
      output: { ...sourceThread.output },
      nodeResults: sourceThread.nodeResults.map(h => ({ ...h })),
      errors: [],
      threadType: "TRIGGERED_SUBWORKFLOW",
      triggeredSubworkflowContext: {
        parentThreadId: sourceThread.id,
        childThreadIds: [],
        triggeredSubworkflowId: "",
      },
    };

    // Create ExecutionState
    const executionState = new ExecutionState();

    // Create WorkflowExecutionEntity (without ConversationManager)
    const copiedThreadEntity = new WorkflowExecutionEntity(copiedThread, executionState);

    // Create ConversationSession (clone from source message history)
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      threadId: copiedThread.id,
      workflowId: copiedThread.workflowId,
      initialMessages: sourceThreadEntity.messageHistoryManager.getMessages(),
    });
    conversationManager.setContext(copiedThread.workflowId, copiedThread.id);

    // Create ThreadStateCoordinator
    const stateCoordinator = new ThreadStateCoordinator({
      threadEntity: copiedThreadEntity,
      conversationManager,
    });

    logger.debug("Thread copy created", { copiedThreadId, sourceThreadId: sourceThread.id });

    return {
      threadEntity: copiedThreadEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Create a fork sub-WorkflowExecutionEntity
   *
   * @param parentThreadEntity Parent WorkflowExecutionEntity
   * @param forkConfig Fork configuration
   * @returns ThreadBuildResult containing WorkflowExecutionEntity, ThreadStateCoordinator, and ConversationSession
   */
  async createFork(
    parentThreadEntity: WorkflowExecutionEntity,
    forkConfig: { forkId: string; forkPathId?: string; startNodeId?: string },
  ): Promise<ThreadBuildResult> {
    const parentThread = parentThreadEntity.getThread();
    const forkThreadId = generateId();

    logger.info("Creating fork thread", {
      parentThreadId: parentThread.id,
      forkThreadId,
      forkId: forkConfig.forkId,
      forkPathId: forkConfig.forkPathId,
    });

    const now = getCurrentTimestamp();

    // Separate thread variables from global variables
    const threadVariables = [];

    for (const variable of parentThread.variables) {
      if (variable.scope === "thread") {
        threadVariables.push({ ...variable });
      }
      // Global variables are not copied to child threads; instead, they are shared through references
    }

    const forkThread: Thread = {
      id: forkThreadId,
      workflowId: parentThread.workflowId,
      workflowVersion: parentThread.workflowVersion,
      currentNodeId: forkConfig.startNodeId || parentThread.currentNodeId,
      graph: parentThread.graph,
      variables: threadVariables,
      // 4-level scopes: global is shared via references, thread creates deep copies, while local and loop scopes clear their contents upon exit
      variableScopes: {
        global: parentThread.variableScopes.global,
        thread: { ...parentThread.variableScopes.thread },
        local: [],
        loop: [],
      },
      input: { ...parentThread.input },
      output: {},
      nodeResults: [],
      errors: [],
      threadType: "FORK_JOIN",
      forkJoinContext: {
        forkId: forkConfig.forkId,
        forkPathId: forkConfig.forkPathId || forkConfig.forkId,
      },
    };

    // Create ExecutionState
    const executionState = new ExecutionState();

    // Create WorkflowExecutionEntity (without ConversationManager)
    const forkThreadEntity = new WorkflowExecutionEntity(forkThread, executionState);

    // Create ConversationSession (clone from parent message history)
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      threadId: forkThread.id,
      workflowId: forkThread.workflowId,
      initialMessages: parentThreadEntity.messageHistoryManager.getMessages(),
    });
    conversationManager.setContext(forkThread.workflowId, forkThread.id);

    // Create ThreadStateCoordinator
    const stateCoordinator = new ThreadStateCoordinator({
      threadEntity: forkThreadEntity,
      conversationManager,
    });

    logger.debug("Fork thread created", { forkThreadId, parentThreadId: parentThread.id });

    return {
      threadEntity: forkThreadEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Clear cache
   */
  clearCache(): void {
    this.threadTemplates.clear();
  }

  /**
   * Invalidate cache for specified workflow
   *
   * @param workflowId Workflow ID
   */
  invalidateWorkflow(workflowId: string): void {
    // Thread templates related to workflow
    for (const [templateId, template] of this.threadTemplates.entries()) {
      if (template.getWorkflowId() === workflowId) {
        this.threadTemplates.delete(templateId);
      }
    }
  }
}
