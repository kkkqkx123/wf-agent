/**
 * NodeExecutionCoordinator - Node Execution Coordinator
 * Responsible for coordinating the execution process of nodes, including event triggering, Hook execution, and subgraph processing.
 *
 * Responsibilities:
 * - Coordinate the core logic of node execution
 * - Handle subgraph boundaries (entry/exit)
 * - Execute nodes (including LLM nodes, user interaction nodes, and regular nodes)
 * - Trigger node events
 * - Execute node Hooks
 *
 * Design Principles:
 * - Coordinate the execution of various components to complete node tasks
 * - Do not implement specific execution logic directly
 * - Provide clear interfaces for node execution
 */

import type { ThreadEntity } from "../../entities/thread-entity.js";
import type { Node } from "@wf-agent/types";
import type {
  NodeExecutionResult,
  Event,
  CheckpointConfig,
  UserInteractionHandler,
  HumanRelayHandler,
  NodeFailedEvent,
} from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { InterruptionState } from "../../../core/types/interruption-state.js";
import type { GraphNavigator } from "../../graph-builder/graph-navigator.js";
import type { ThreadRegistry } from "../../stores/thread-registry.js";
import type { ThreadStateCoordinator } from "../../state-managers/thread-state-coordinator.js";
import { LLMExecutionCoordinator } from "./llm-execution-coordinator.js";
import { SDKError } from "@wf-agent/types";
import {
  enterSubgraph,
  exitSubgraph,
  getSubgraphInput,
  getSubgraphOutput,
} from "../handlers/subgraph-handler.js";

import { executeHook } from "../handlers/hook-handlers/hook-handler.js";
import { handleErrorWithContext } from "../../../core/utils/error-utils.js";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";
import { emit } from "../../../core/utils/event/event-emitter.js";
import { getNodeHandler } from "../handlers/node-handlers/index.js";
import { SUBGRAPH_METADATA_KEYS, SubgraphBoundaryType } from "@wf-agent/types";
import type { CheckpointDependencies } from "../../checkpoint/utils/checkpoint-utils.js";
import { createCheckpoint } from "../../checkpoint/utils/checkpoint-utils.js";
import {
  resolveCheckpointConfig,
  buildNodeCheckpointLayers,
} from "../../checkpoint/utils/config-resolver.js";
import {
  buildThreadPausedEvent,
  buildThreadCancelledEvent,
  buildNodeStartedEvent,
  buildNodeCompletedEvent,
  buildNodeFailedEvent,
  buildSubgraphStartedEvent,
  buildSubgraphCompletedEvent,
} from "../utils/event/index.js";
import type { InterruptionDetector } from "../interruption-detector.js";
import {
  checkInterruption,
  shouldContinue,
  getInterruptionDescription,
} from "@wf-agent/common-utils";
import { NodeHandlerContextFactory } from "../factories/node-handler-context-factory.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "node-execution-coordinator" });

/**
 * Node Execution Coordinator Configuration
 */
export interface NodeExecutionCoordinatorConfig {
  // Core Dependencies (Required)
  /** Event Manager */
  eventManager: EventRegistry;
  /** LLM Execution Coordinator */
  llmCoordinator: LLMExecutionCoordinator;
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** Interrupt Manager */
  interruptionManager: InterruptionState;
  /** Image Navigator */
  navigator: GraphNavigator;

  // Checkpoint-related (optional)
  /** Checkpoint dependencies (optional) */
  checkpointDependencies?: CheckpointDependencies;
  /** Global checkpoint configuration (optional) */
  globalCheckpointConfig?: CheckpointConfig;

  // Interrupt detection-related (optional)
  /** Thread Registry (optional) */
  threadRegistry?: ThreadRegistry;
  /** Interrupt Detector (Optional) */
  interruptionDetector?: InterruptionDetector;

  // Processor context factory configuration
  /** User Interaction Processor (optional) */
  userInteractionHandler?: UserInteractionHandler;
  /** Manual Relay Processor (optional) */
  humanRelayHandler?: HumanRelayHandler;
  /** Tool Context Store (optional) */
  toolContextStore?: unknown;
  /** Tool Services (Optional) */
  toolService?: unknown;
  /** Agent Loop Executor Factory (optional) */
  agentLoopExecutorFactory?: unknown;
}

/**
 * Node Execution Coordinator
 *
 * Design Notes:
 * - Adopt a flattened dependency management approach to simplify access paths.
 * - Core dependencies (such as eventManager, llmCoordinator, etc.) are required.
 * - Use HandlerContextFactory to create handler contexts to avoid excessive responsibility on any single component.
 */
export class NodeExecutionCoordinator {
  // Core Dependencies (Required)
  private eventManager: EventRegistry;
  private interruptionManager: InterruptionState;
  private navigator: GraphNavigator;

  // Checkpoint-related (optional)
  private checkpointDependencies?: CheckpointDependencies;
  private globalCheckpointConfig?: CheckpointConfig;

  // Interrupt detection-related (optional)
  private threadRegistry?: ThreadRegistry;
  private interruptionDetector?: InterruptionDetector;

  // Processor Context Factory
  private handlerContextFactory: NodeHandlerContextFactory;

  constructor(config: NodeExecutionCoordinatorConfig) {
    // Core Dependencies
    this.eventManager = config.eventManager;
    this.interruptionManager = config.interruptionManager;
    this.navigator = config.navigator;

    // Checkpoint-related
    this.checkpointDependencies = config.checkpointDependencies;
    this.globalCheckpointConfig = config.globalCheckpointConfig;

    // Interrupt detection-related
    this.threadRegistry = config.threadRegistry;
    this.interruptionDetector = config.interruptionDetector;

    // Create a processor context factory
    this.handlerContextFactory = new NodeHandlerContextFactory({
      eventManager: config.eventManager,
      llmCoordinator: config.llmCoordinator,
      conversationManager: config.conversationManager,
      userInteractionHandler: config.userInteractionHandler,
      humanRelayHandler: config.humanRelayHandler,
      toolContextStore: config.toolContextStore,
      toolService: config.toolService,
      agentLoopExecutorFactory: config.agentLoopExecutorFactory,
      threadRegistry: config.threadRegistry,
    });
  }

  /**
   * Check if it has been aborted
   *
   * @param threadId Thread ID
   * @returns Whether it has been aborted
   */
  isAborted(threadId: string): boolean {
    if (this.interruptionDetector) {
      return this.interruptionDetector.isAborted(threadId);
    }

    const threadContext = this.threadRegistry?.get(threadId);
    if (!threadContext) {
      return false;
    }

    return threadContext.getAbortSignal().aborted;
  }

  /**
   * Handle Interrupt Operations
   *
   * @param threadId Thread ID
   * @param nodeId Node ID
   * @param type Interrupt Type (PAUSE or STOP)
   */
  async handleInterruption(
    threadId: string,
    nodeId: string,
    type: "PAUSE" | "STOP",
  ): Promise<void> {
    logger.info("Handling interruption", { threadId, nodeId, type });

    if (!this.threadRegistry) {
      logger.debug("ThreadRegistry not available, skipping interruption handling", { threadId });
      return;
    }

    const threadContext = this.threadRegistry.get(threadId);
    if (!threadContext) {
      logger.warn("ThreadContext not found for interruption", { threadId, nodeId });
      return;
    }

    // Create an interrupt checkpoint
    if (this.checkpointDependencies) {
      try {
        await createCheckpoint(
          {
            threadId,
            nodeId,
            description: `Thread ${type.toLowerCase()} at node: ${nodeId}`,
            metadata: {
              customFields: {
                interruptionType: type,
                interruptedAt: now(),
              },
            },
          },
          this.checkpointDependencies,
        );
        logger.debug("Interruption checkpoint created", { threadId, nodeId, type });
      } catch (error) {
        // `threadContext` is of the `ThreadEntity` type.
        await handleErrorWithContext(
          this.eventManager,
          getErrorOrNew(error) as SDKError,
          threadContext,
          "CREATE_INTERRUPTION_CHECKPOINT",
        );
        throw error;
      }
    }

    // Trigger the corresponding event.
    if (type === "PAUSE") {
      threadContext.setStatus("PAUSED");
      const pausedEvent = buildThreadPausedEvent(threadContext);
      await emit(this.eventManager, pausedEvent);
      logger.info("Thread paused event emitted", { threadId, nodeId });
    } else if (type === "STOP") {
      threadContext.setStatus("CANCELLED");
      threadContext.state.cancel();
      const cancelledEvent = buildThreadCancelledEvent(threadContext, "user_requested");
      await emit(this.eventManager, cancelledEvent);
      logger.info("Thread cancelled event emitted", { threadId, nodeId });
    }
  }

  /**
   * Execute Node
   * @param threadEntity Thread entity
   * @param node Node definition
   * @returns Node execution result
   */
  async executeNode(threadEntity: ThreadEntity, node: Node): Promise<NodeExecutionResult> {
    const nodeId = node.id;
    const nodeType = node.type;
    const threadId = threadEntity.id;
    const abortSignal = this.interruptionManager.getAbortSignal();

    logger.debug("Starting node execution", { threadId, nodeId, nodeType, nodeName: node.name });

    // Use the return value tagging system to check for interruptions.
    const interruption = checkInterruption(abortSignal);

    if (!shouldContinue(interruption)) {
      logger.info("Node execution interrupted", {
        threadId,
        nodeId,
        interruptionType: interruption.type,
      });
      // If interrupted, handle the interruption (create checkpoints, trigger events).
      const interruptionType = interruption.type === "paused" ? "PAUSE" : "STOP";
      await this.handleInterruption(threadId, nodeId, interruptionType);

      // Return a result with the status CANCELLED without throwing any errors.
      const cancelledResult: NodeExecutionResult = {
        nodeId,
        nodeType,
        status: "CANCELLED",
        step: threadEntity.getNodeResults().length + 1,
        error: getInterruptionDescription(interruption),
        startTime: now(),
        endTime: now(),
        executionTime: 0,
      };

      threadEntity.addNodeResult(cancelledResult);
      return cancelledResult;
    }

    // Get the GraphNode to check the boundary information.
    const graphNode = this.navigator.getGraph().getNode(nodeId);

    // Check if it is a boundary node of a subgraph.
    if (graphNode?.internalMetadata?.[SUBGRAPH_METADATA_KEYS.BOUNDARY_TYPE]) {
      logger.debug("Handling subgraph boundary", {
        threadId,
        nodeId,
        boundaryType: graphNode.internalMetadata[SUBGRAPH_METADATA_KEYS.BOUNDARY_TYPE],
      });
      await this.handleSubgraphBoundary(threadEntity, graphNode);
    }

    try {
      // Step 1: Trigger the node start event
      const nodeStartedEvent = threadEntity.buildEvent(buildNodeStartedEvent, {
        nodeType,
      });
      await emit(this.eventManager, nodeStartedEvent);

      // Step 2: Create a checkpoint before node execution (if configured)
      if (this.checkpointDependencies) {
        const context = {
          triggerType: "NODE_BEFORE_EXECUTE" as const,
          nodeId,
        };
        const layers = buildNodeCheckpointLayers(this.globalCheckpointConfig, node, context);
        const configResult = resolveCheckpointConfig(layers, context);

        if (configResult.shouldCreate) {
          logger.debug("Creating checkpoint before node execution", { threadId, nodeId });
          try {
            await createCheckpoint(
              {
                threadId: threadEntity.id,
                nodeId,
                description: configResult.description || `Before node: ${node.name}`,
              },
              this.checkpointDependencies,
            );
          } catch (error) {
            await handleErrorWithContext(
              this.eventManager,
              getErrorOrNew(error) as SDKError,
              threadEntity,
              "CREATE_CHECKPOINT",
            );
            throw error;
          }
        }
      }

      // Step 3: Execute the BEFORE_EXECUTE type of Hook
      if (node.hooks && node.hooks.length > 0) {
        logger.debug("Executing BEFORE_EXECUTE hooks", {
          threadId,
          nodeId,
          hookCount: node.hooks.length,
        });
        await executeHook(
          {
            thread: threadEntity.getThread(),
            threadEntity,
            node,
            checkpointDependencies: this.checkpointDependencies,
          },
          "BEFORE_EXECUTE",
          event => this.eventManager.emit(event),
        );
      }

      // Step 4: Execute node logic
      logger.debug("Executing node logic", { threadId, nodeId, nodeType });
      const nodeResult = await this.executeNodeLogic(threadEntity, node);

      // Step 5: Record the results of node execution
      threadEntity.addNodeResult(nodeResult);

      // Step 6: Execute the Hook of type AFTER_EXECUTE
      if (node.hooks && node.hooks.length > 0) {
        logger.debug("Executing AFTER_EXECUTE hooks", {
          threadId,
          nodeId,
          hookCount: node.hooks.length,
        });
        await executeHook(
          {
            thread: threadEntity.getThread(),
            threadEntity,
            node,
            result: nodeResult,
            checkpointDependencies: this.checkpointDependencies,
          },
          "AFTER_EXECUTE",
          event => this.eventManager.emit(event),
        );
      }

      // Step 7: Create a checkpoint after the node has executed (if configured).
      if (this.checkpointDependencies) {
        const context = {
          triggerType: "NODE_AFTER_EXECUTE" as const,
          nodeId,
        };
        const layers = buildNodeCheckpointLayers(this.globalCheckpointConfig, node, context);
        const configResult = resolveCheckpointConfig(layers, context);

        if (configResult.shouldCreate) {
          logger.debug("Creating checkpoint after node execution", { threadId, nodeId });
          try {
            await createCheckpoint(
              {
                threadId: threadEntity.id,
                nodeId,
                description: configResult.description || `After node: ${node.name}`,
              },
              this.checkpointDependencies,
            );
          } catch (error) {
            await handleErrorWithContext(
              this.eventManager,
              getErrorOrNew(error) as SDKError,
              threadEntity,
              "CREATE_CHECKPOINT_AFTER",
            );
            throw error;
          }
        }
      }

      // Step 8: Trigger the node completion event
      if (nodeResult.status === "COMPLETED") {
        const nodeCompletedEvent = threadEntity.buildEvent(buildNodeCompletedEvent, {
          output: threadEntity.getOutput(),
          executionTime: nodeResult.executionTime || 0,
        });
        await emit(this.eventManager, nodeCompletedEvent);
        logger.debug("Node execution completed", {
          threadId,
          nodeId,
          executionTime: nodeResult.executionTime,
        });
      } else if (nodeResult.status === "FAILED") {
        const nodeFailedEvent = threadEntity.buildEvent(buildNodeFailedEvent as any, {
          error: getErrorOrNew(nodeResult.error),
        });
        await emit(this.eventManager, nodeFailedEvent as Event);
        logger.warn("Node execution failed", {
          threadId,
          nodeId,
          error: getErrorOrNew(nodeResult.error),
        });
      }

      return nodeResult;
    } catch (error) {
      // Use a unified error handler to handle errors.
      const enhancedError = await handleErrorWithContext(
        this.eventManager,
        getErrorOrNew(error) as SDKError,
        threadEntity,
        "EXECUTE_NODE",
      );

      // Handling node execution errors
      const errorResult: NodeExecutionResult = {
        nodeId,
        nodeType,
        status: "FAILED",
        step: threadEntity.getNodeResults().length + 1,
        error: enhancedError,
        startTime: now(),
        endTime: now(),
        executionTime: 0,
      };

      threadEntity.addNodeResult(errorResult);

      const nodeFailedEvent = threadEntity.buildEvent(buildNodeFailedEvent as any, {
        error: enhancedError,
      });
      await emit(this.eventManager, nodeFailedEvent as Event);

      return errorResult;
    }
  }

  /**
   * Handle subgraph boundaries
   * @param threadEntity Thread entity
   * @param graphNode Graph node
   */
  private async handleSubgraphBoundary(
    threadEntity: ThreadEntity,
    graphNode: {
      internalMetadata?: Record<string, unknown>;
      workflowId: string;
      parentWorkflowId?: string;
    },
  ): Promise<void> {
    const boundaryType = graphNode.internalMetadata?.[
      SUBGRAPH_METADATA_KEYS.BOUNDARY_TYPE
    ] as SubgraphBoundaryType;
    const threadId = threadEntity.id;

    if (boundaryType === "entry") {
      logger.info("Entering subgraph", {
        threadId,
        subgraphId: graphNode.workflowId,
        parentWorkflowId: graphNode.parentWorkflowId,
      });
      // Enter the subgraph
      const input = getSubgraphInput(threadEntity);
      await enterSubgraph(threadEntity, graphNode.workflowId, graphNode.parentWorkflowId!, input);

      // Trigger the start event of the subgraph
      const subgraphStartedEvent = threadEntity.buildEvent(buildSubgraphStartedEvent, {
        subgraphId: graphNode.workflowId,
        parentWorkflowId: graphNode.parentWorkflowId!,
        input,
      });
      await emit(this.eventManager, subgraphStartedEvent);
    } else if (boundaryType === "exit") {
      // Exit the subgraph
      const subgraphContext = threadEntity.getCurrentSubgraphContext();
      if (subgraphContext) {
        const output = getSubgraphOutput(threadEntity);

        logger.info("Exiting subgraph", {
          threadId,
          subgraphId: subgraphContext.workflowId,
          executionTime: diffTimestamp(subgraphContext.startTime, now()),
        });

        // Trigger the completion event of the subgraph.
        const subgraphCompletedEvent = threadEntity.buildEvent(buildSubgraphCompletedEvent, {
          subgraphId: subgraphContext.workflowId,
          output,
          executionTime: diffTimestamp(subgraphContext.startTime, now()),
        });
        await emit(this.eventManager, subgraphCompletedEvent);

        await exitSubgraph(threadEntity);
      }
    }
  }

  /**
   * Execute node logic
   * @param threadContext Thread context
   * @param node Node definition
   * @returns Node execution result
   */
  private async executeNodeLogic(
    threadEntity: ThreadEntity,
    node: Node,
  ): Promise<NodeExecutionResult> {
    const startTime = now();

    // 1. Execute using the Node Handler function (the configuration has been statically verified during workflow registration).
    const handler = getNodeHandler(node.type);

    // 2. Use the factory to create the processor context.
    const handlerContext = this.handlerContextFactory.createHandlerContext(node, threadEntity);

    // 3. Execute the processor
    const output = await handler(threadEntity, node, handlerContext);

    // 4. Constructing the execution results
    const endTime = now();
    const outputStatus = (
      output as
        | { status?: "PENDING" | "RUNNING" | "COMPLETED" | "FAILED" | "SKIPPED" | "CANCELLED" }
        | undefined
    )?.status;
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: outputStatus || "COMPLETED",
      step: threadEntity.getNodeResults().length + 1,
      startTime,
      endTime,
      executionTime: diffTimestamp(startTime, endTime),
    };
  }
}
