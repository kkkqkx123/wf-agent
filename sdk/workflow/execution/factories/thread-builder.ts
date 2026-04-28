/**
 * ThreadBuilder - Thread Builder
 *
 * Responsible for retrieving WorkflowDefinition from WorkflowRegistry and creating ThreadEntity instances.
 * Provides support for thread template caching and deep copying, as well as the use of preprocessed graphs and graph navigation.
 *
 * Note: ThreadBuilder is used directly for thread creation, not wrapped as ThreadFactory.
 * The naming difference from AgentLoopFactory reflects the different execution models.
 *
 * Migration Note:
 * - ThreadEntity no longer holds ConversationManager directly
 * - ThreadStateCoordinator is created separately to manage state
 * - This eliminates data redundancy and synchronization issues
 */

import type { PreprocessedGraph } from "@wf-agent/types";
import type { Thread, ThreadOptions, ThreadStatus } from "@wf-agent/types";
import { ThreadEntity } from "../../entities/workflow-execution-entity.js";
import { ExecutionState } from "../../state-managers/execution-state.js";
import { ThreadStateCoordinator } from "../../state-managers/workflow-state-coordinator.js";
import { generateId, now as getCurrentTimestamp } from "@wf-agent/common-utils";
import { ExecutionError, RuntimeValidationError } from "@wf-agent/types";
import type { GraphRegistry } from "../../stores/workflow-graph-registry.js";
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
 * Contains both ThreadEntity and ThreadStateCoordinator
 */
export interface ThreadBuildResult {
  threadEntity: ThreadEntity;
  stateCoordinator: WorkflowStateCoordinator;
  conversationManager: ConversationSession;
}

/**
 * ThreadBuilder - Thread Builder
 *
 * Factory class for creating ThreadEntity instances from workflows.
 */
export class ThreadBuilder {
  private threadTemplates: Map<string, ThreadEntity> = new Map();

  constructor() {}

  /**
   * Get graph registry (from DI container)
   */
  private getGraphRegistry(): WorkflowGraphRegistry {
    const container = getContainer();
    return container.get(Identifiers.GraphRegistry) as GraphRegistry;
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
   * Get workflow from WorkflowRegistry and construct ThreadEntity
   *
   * @param workflowId Workflow ID
   * @param options Thread options
   * @returns ThreadBuildResult containing ThreadEntity, ThreadStateCoordinator, and ConversationSession
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

    logger.info("Thread built successfully", { threadId: result.threadEntity.id, workflowId });

    return result;
  }

  /**
   * Build ThreadEntity from PreprocessedGraph (internal method)
   *
   * @param preprocessedGraph Preprocessed graph
   * @param options Thread options
   * @returns ThreadBuildResult containing ThreadEntity, ThreadStateCoordinator, and ConversationSession
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

    // Step 6: Create ThreadEntity (without ConversationManager)
    const threadEntity = new ThreadEntity(thread, executionState);

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
   * Build ThreadEntity from cached template
   *
   * @param templateId Template ID
   * @returns ThreadBuildResult containing ThreadEntity, ThreadStateCoordinator, and ConversationSession
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
   * Create a copy of ThreadEntity
   *
   * @param sourceThreadEntity Source ThreadEntity
   * @returns ThreadBuildResult containing ThreadEntity, ThreadStateCoordinator, and ConversationSession
   */
  async createCopy(sourceThreadEntity: ThreadEntity): Promise<ThreadBuildResult> {
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

    // Create ThreadEntity (without ConversationManager)
    const copiedThreadEntity = new ThreadEntity(copiedThread, executionState);

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
   * Create a fork sub-ThreadEntity
   *
   * @param parentThreadEntity Parent ThreadEntity
   * @param forkConfig Fork configuration
   * @returns ThreadBuildResult containing ThreadEntity, ThreadStateCoordinator, and ConversationSession
   */
  async createFork(
    parentThreadEntity: ThreadEntity,
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

    // Create ThreadEntity (without ConversationManager)
    const forkThreadEntity = new ThreadEntity(forkThread, executionState);

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
