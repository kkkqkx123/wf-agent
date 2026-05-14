/**
 * NodeExecutionCoordinator - Node Execution Coordinator
 * Responsible for coordinating the execution process of nodes, including event triggering, Hook execution, and subworkflow processing.
 *
 * Responsibilities:
 * - Coordinate the core logic of node execution
 * - Handle subworkflow boundaries (entry/exit)
 * - Execute nodes (including LLM nodes, user interaction nodes, and regular nodes)
 * - Trigger node events
 * - Execute node Hooks
 *
 * Design Principles:
 * - Coordinate the execution of various components to complete node tasks
 * - Do not implement specific execution logic directly
 * - Provide clear interfaces for node execution
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { RuntimeNode, WorkflowNode, StaticNode } from "@wf-agent/types";
import type {
  NodeExecutionResult,
  Event,
  CheckpointConfig,
  UserInteractionHandler,
  HumanRelayHandler,
} from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { GlobalContext } from "../../../core/global-context.js";
import type { InterruptionState } from "../../../core/types/interruption-state.js";
import type { WorkflowNavigator } from "../../builder/workflow-navigator.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
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
import { isSubgraphStartNode, isSubgraphEndNode } from "@wf-agent/types";
import type { CheckpointDependencies } from "../../checkpoint/utils/checkpoint-utils.js";
import { createCheckpoint } from "../../checkpoint/utils/checkpoint-utils.js";
import {
  resolveCheckpointConfig,
  buildNodeCheckpointLayers,
} from "../../checkpoint/utils/config-resolver.js";
import {
  buildWorkflowExecutionPausedEvent,
  buildWorkflowExecutionCancelledEvent,
  buildNodeStartedEvent,
  buildNodeCompletedEvent,
  buildNodeFailedEvent,
  buildSubgraphStartedEvent,
  buildSubgraphCompletedEvent,
} from "../utils/event/index.js";
import type { InterruptionDetector } from "../interruption-detector.js";
import {
  getWorkflowInterruptionDescription,
  executeWithInterruptionHandling,
} from "../../../core/utils/interruption/index.js";
import { NodeHandlerContextFactory } from "../factories/node-handler-context-factory.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { MetricsRegistry } from "../../../core/metrics/metrics-registry.js";

const logger = createContextualLogger({ operation: "node-execution-coordinator" });

/**
 * Node Execution Coordinator Configuration
 */
export interface NodeExecutionCoordinatorConfig {
  // Core Dependencies (Required)
  /** Global Context */
  globalContext: GlobalContext;
  /** Event Manager */
  eventManager: EventRegistry;
  /** LLM Execution Coordinator */
  llmCoordinator: LLMExecutionCoordinator;
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** Interrupt Manager */
  interruptionManager: InterruptionState;
  /** Workflow Navigator */
  navigator: WorkflowNavigator;

  // Checkpoint-related (optional)
  /** Checkpoint dependencies (optional) */
  checkpointDependencies?: CheckpointDependencies;
  /** Global checkpoint configuration (optional) */
  globalCheckpointConfig?: CheckpointConfig;

  // Interrupt detection-related (optional)
  /** Workflow Execution Registry (optional) */
  workflowExecutionRegistry?: WorkflowExecutionRegistry;
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
  private globalContext: GlobalContext;
  private eventManager: EventRegistry;
  private interruptionManager: InterruptionState;
  private navigator: WorkflowNavigator;

  // Checkpoint-related (optional)
  private checkpointDependencies?: CheckpointDependencies;
  private globalCheckpointConfig?: CheckpointConfig;

  // Interrupt detection-related (optional)
  private workflowExecutionRegistry?: WorkflowExecutionRegistry;
  private interruptionDetector?: InterruptionDetector;

  // Processor Context Factory
  private handlerContextFactory: NodeHandlerContextFactory;

  // Metrics Registry
  private metricsRegistry: MetricsRegistry;

  constructor(config: NodeExecutionCoordinatorConfig) {
    // Core Dependencies
    this.globalContext = config.globalContext;
    this.eventManager = config.eventManager;
    this.interruptionManager = config.interruptionManager;
    this.navigator = config.navigator;

    // Checkpoint-related
    this.checkpointDependencies = config.checkpointDependencies;
    this.globalCheckpointConfig = config.globalCheckpointConfig;

    // Interrupt detection-related
    this.workflowExecutionRegistry = config.workflowExecutionRegistry;
    this.interruptionDetector = config.interruptionDetector;

    // Get metrics registry from global context
    this.metricsRegistry = this.globalContext.metricsRegistry;

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
      workflowExecutionRegistry: config.workflowExecutionRegistry,
    });
  }

  /**
   * Check if it has been aborted
   *
   * @param executionId Workflow Execution ID
   * @returns Whether it has been aborted
   */
  isAborted(executionId: string): boolean {
    if (this.interruptionDetector) {
      return this.interruptionDetector.isAborted(executionId);
    }

    const workflowExecutionContext = this.workflowExecutionRegistry?.get(executionId);
    if (!workflowExecutionContext) {
      return false;
    }

    return workflowExecutionContext.getAbortSignal().aborted;
  }

  /**
   * Handle Interrupt Operations
   *
   * @param executionId Workflow Execution ID
   * @param nodeId Node ID
   * @param type Interrupt Type (PAUSE or STOP)
   */
  async handleInterruption(
    workflowExecutionId: string,
    nodeId: string,
    type: "PAUSE" | "STOP",
  ): Promise<void> {
    logger.info("Handling interruption", { executionId: workflowExecutionId, nodeId, type });

    if (!this.workflowExecutionRegistry) {
      logger.debug("WorkflowExecutionRegistry not available, skipping interruption handling", { executionId: workflowExecutionId });
      return;
    }

    const workflowExecutionContext = this.workflowExecutionRegistry.get(workflowExecutionId);
    if (!workflowExecutionContext) {
      logger.warn("WorkflowExecutionContext not found for interruption", { executionId: workflowExecutionId, nodeId });
      return;
    }

    // Create an interrupt checkpoint
    let checkpointId: string | undefined;
    if (this.checkpointDependencies) {
      try {
        checkpointId = await createCheckpoint(
          {
            workflowExecutionId,
            nodeId,
            description: `Workflow execution ${type.toLowerCase()} at node: ${nodeId}`,
            metadata: {
              customFields: {
                interruptionType: type,
                interruptedAt: now(),
              },
            },
          },
          this.checkpointDependencies,
        );
        logger.debug("Interruption checkpoint created", { executionId: workflowExecutionId, nodeId, type, checkpointId });
      } catch (error) {
        // `workflowExecutionContext` is of the `WorkflowExecutionEntity` type.
        await handleErrorWithContext(
          this.eventManager,
          getErrorOrNew(error) as SDKError,
          workflowExecutionContext,
          "CREATE_INTERRUPTION_CHECKPOINT",
        );
        throw error;
      }
    }

    // Trigger the corresponding event with rich context
    if (type === "PAUSE") {
      workflowExecutionContext.setStatus("PAUSED");
      const pausedEvent = buildWorkflowExecutionPausedEvent(workflowExecutionContext, {
        nodeId,
        completedNodes: workflowExecutionContext.getNodeResults().length,
        pendingToolsCancelled: true,
        checkpointCreated: !!checkpointId,
        checkpointId,
      });
      await emit(this.eventManager, pausedEvent);
      logger.info("Workflow execution paused event emitted", { executionId: workflowExecutionId, nodeId });
    } else if (type === "STOP") {
      workflowExecutionContext.setStatus("CANCELLED");
      workflowExecutionContext.state.cancel();
      const cancelledEvent = buildWorkflowExecutionCancelledEvent(
        workflowExecutionContext,
        "user_requested",
        {
          nodeId,
          completedNodes: workflowExecutionContext.getNodeResults().length,
          pendingToolsCancelled: true,
          checkpointCreated: !!checkpointId,
          checkpointId,
        },
      );
      await emit(this.eventManager, cancelledEvent);
      logger.info("Workflow execution cancelled event emitted", { executionId: workflowExecutionId, nodeId });
    }
  }

  /**
   * Execute Node
   * @param workflowExecutionEntity Workflow execution entity
   * @param node Node definition
   * @param options Execution options (including abortSignal)
   * @returns Node execution result
   */
  async executeNode(
    workflowExecutionEntity: WorkflowExecutionEntity,
    node: WorkflowNode | RuntimeNode,
    options?: { abortSignal?: AbortSignal }
  ): Promise<NodeExecutionResult> {
    const nodeId = node.id;
    const nodeType = node.type;
    const executionId = workflowExecutionEntity.id;
    const signal = options?.abortSignal ?? this.interruptionManager.getAbortSignal();
    const workflowId = workflowExecutionEntity.getWorkflowId();

    logger.debug("Starting node execution", { executionId, nodeId, nodeType, nodeName: (node as WorkflowNode).name || ((node as RuntimeNode) as WorkflowNode).originalNode?.name });

    // Record node execution start in metrics
    this.metricsRegistry.getCollectors().node.recordNodeExecutionStart(
      nodeId,
      nodeType,
      workflowId
    );

    return await executeWithInterruptionHandling(
      async (effectiveSignal) => {
        // Get the GraphNode to check the boundary information.
        const graphNode = this.navigator.getGraph().getNode(nodeId);

        // Check if it is a boundary node of a subgraph using type guards.
        if (graphNode && (isSubgraphStartNode(graphNode) || isSubgraphEndNode(graphNode))) {
          logger.debug("Handling subgraph boundary", {
            executionId,
            nodeId,
            nodeType: graphNode.type,
          });
          await this.handleSubgraphBoundary(workflowExecutionEntity, graphNode);
        }

        try {
          // Step 1: Trigger the node start event
          const nodeStartedEvent = workflowExecutionEntity.buildEvent(buildNodeStartedEvent, {
            nodeId,
            nodeType,
          });
          await emit(this.eventManager, nodeStartedEvent);

          // Step 2: Create a checkpoint before node execution (if configured)
          if (this.checkpointDependencies) {
            const context = {
              triggerType: "NODE_BEFORE_EXECUTE" as const,
              nodeId,
            };
            // Convert to StaticNode for checkpoint config resolution
            const staticNode = ('originalNode' in node ? (node as WorkflowNode).originalNode : node) as StaticNode | undefined;
            const layers = buildNodeCheckpointLayers(this.globalCheckpointConfig, staticNode, context);
            const configResult = resolveCheckpointConfig(layers, context);

            if (configResult.shouldCreate) {
              logger.debug("Creating checkpoint before node execution", { executionId, nodeId });
              try {
                await createCheckpoint(
                  {
                    workflowExecutionId: workflowExecutionEntity.id,
                    nodeId,
                    description: configResult.description || `Before node: ${(node as WorkflowNode).name || ((node as RuntimeNode) as WorkflowNode).originalNode?.name}`,
                  },
                  this.checkpointDependencies,
                );
              } catch (error) {
                await handleErrorWithContext(
                  this.eventManager,
                  getErrorOrNew(error) as SDKError,
                  workflowExecutionEntity,
                  "CREATE_CHECKPOINT",
                );
                throw error;
              }
            }
          }

          // Step 3: Execute the BEFORE_EXECUTE type of Hook
          if (node.hooks && node.hooks.length > 0) {
            logger.debug("Executing BEFORE_EXECUTE hooks", {
              executionId,
              nodeId,
              hookCount: node.hooks.length,
            });
            await executeHook(
              {
                workflowExecution: workflowExecutionEntity.getWorkflowExecutionData(),
                workflowExecutionEntity,
                node: ('originalNode' in node ? (node as WorkflowNode).originalNode : node) as StaticNode,
                checkpointDependencies: this.checkpointDependencies,
              },
              "BEFORE_EXECUTE",
              event => this.eventManager.emit(event),
            );
          }

          // Step 4: Execute node logic
          logger.debug("Executing node logic", { executionId, nodeId, nodeType });
          const nodeStartTime = now();
          const nodeResult = await this.executeNodeLogic(workflowExecutionEntity, node, effectiveSignal);
          const nodeDuration = diffTimestamp(nodeStartTime, now());

          // Record node execution completion in metrics
          this.metricsRegistry.getCollectors().node.recordNodeExecution(
            nodeId,
            nodeType,
            workflowId,
            {
              success: nodeResult.status === 'COMPLETED',
              duration: nodeDuration,
              errorType: nodeResult.status === 'FAILED' ? (nodeResult.error instanceof Error ? nodeResult.error.name : 'unknown') : undefined,
            }
          );

          // Step 5: Record the results of node execution
          workflowExecutionEntity.addNodeResult(nodeResult);

          // Step 6: Execute the Hook of type AFTER_EXECUTE
          if (node.hooks && node.hooks.length > 0) {
            logger.debug("Executing AFTER_EXECUTE hooks", {
              executionId,
              nodeId,
              hookCount: node.hooks.length,
            });
            await executeHook(
              {
                workflowExecution: workflowExecutionEntity.getWorkflowExecutionData(),
                workflowExecutionEntity,
                node: ('originalNode' in node ? (node as WorkflowNode).originalNode : node) as StaticNode,
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
            // Convert to StaticNode for checkpoint config resolution
            const staticNode = ('originalNode' in node ? (node as WorkflowNode).originalNode : node) as StaticNode | undefined;
            const layers = buildNodeCheckpointLayers(this.globalCheckpointConfig, staticNode, context);
            const configResult = resolveCheckpointConfig(layers, context);

            if (configResult.shouldCreate) {
              logger.debug("Creating checkpoint after node execution", { executionId, nodeId });
              try {
                await createCheckpoint(
                  {
                    workflowExecutionId: workflowExecutionEntity.id,
                    nodeId,
                    description: configResult.description || `After node: ${(node as WorkflowNode).name || ((node as RuntimeNode) as WorkflowNode).originalNode?.name}`,
                  },
                  this.checkpointDependencies,
                );
              } catch (error) {
                await handleErrorWithContext(
                  this.eventManager,
                  getErrorOrNew(error) as SDKError,
                  workflowExecutionEntity,
                  "CREATE_CHECKPOINT_AFTER",
                );
                throw error;
              }
            }
          }

          // Step 8: Trigger the node completion event
          if (nodeResult.status === "COMPLETED") {
            const nodeCompletedEvent = workflowExecutionEntity.buildEvent(buildNodeCompletedEvent, {
              nodeId,
              output: workflowExecutionEntity.getOutput(),
              executionTime: nodeResult.executionTime || 0,
            });
            await emit(this.eventManager, nodeCompletedEvent);
            logger.debug("Node execution completed", {
              executionId,
              nodeId,
              executionTime: nodeResult.executionTime,
            });
          } else if (nodeResult.status === "FAILED") {
            const nodeFailedEvent = buildNodeFailedEvent({
              executionId: workflowExecutionEntity.id,
              workflowId: workflowExecutionEntity.getWorkflowId(),
              nodeId,
              error: getErrorOrNew(nodeResult.error),
            });
            await emit(this.eventManager, nodeFailedEvent as Event);
            logger.warn("Node execution failed", {
              executionId,
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
            workflowExecutionEntity,
            "EXECUTE_NODE",
          );

          // Handling node execution errors
          const errorResult: NodeExecutionResult = {
            nodeId,
            nodeType,
            status: "FAILED",
            step: workflowExecutionEntity.getNodeResults().length + 1,
            error: enhancedError,
            startTime: now(),
            endTime: now(),
            executionTime: 0,
          };

          workflowExecutionEntity.addNodeResult(errorResult);

          const nodeFailedEvent = buildNodeFailedEvent({
            executionId: workflowExecutionEntity.id,
            workflowId: workflowExecutionEntity.getWorkflowId(),
            nodeId,
            error: enhancedError,
          });
          await emit(this.eventManager, nodeFailedEvent as Event);

          return errorResult;
        }
      },
      signal
    ).then(result => {
      if (!result.success) {
        // Handle interruption - create checkpoints and trigger events
        const interruption = result.interruption;
        const interruptionType = interruption.type === "paused" ? "PAUSE" : "STOP";
        
        // Return a CANCELLED result
        const cancelledResult: NodeExecutionResult = {
          nodeId,
          nodeType,
          status: "CANCELLED",
          step: workflowExecutionEntity.getNodeResults().length + 1,
          error: getWorkflowInterruptionDescription(interruption),
          startTime: now(),
          endTime: now(),
          executionTime: 0,
        };

        workflowExecutionEntity.addNodeResult(cancelledResult);
        return cancelledResult;
      }
      return result.result;
    });
  }

  /**
   * Handle subgraph boundaries
   * @param workflowExecutionEntity Workflow execution entity
   * @param graphNode Graph node
   */
  private async handleSubgraphBoundary(
    workflowExecutionEntity: WorkflowExecutionEntity,
    graphNode: {
      id: string;
      type: string;
      internalMetadata?: Record<string, unknown>;
      workflowId: string;
      parentWorkflowId?: string;
      originalNode?: StaticNode; // Original SUBGRAPH node
    },
  ): Promise<void> {
    const executionId = workflowExecutionEntity.id;

    if (isSubgraphStartNode(graphNode)) {
      logger.info("Entering subgraph", {
        executionId,
        subgraphId: graphNode.workflowId,
        parentWorkflowId: graphNode.parentWorkflowId,
      });
      // Enter the subgraph
      const input = getSubgraphInput(workflowExecutionEntity);
      // Pass the original SUBGRAPH node for message context handling
      if (graphNode.originalNode) {
        await enterSubgraph(workflowExecutionEntity, graphNode.workflowId, graphNode.parentWorkflowId!, input, graphNode.originalNode);
      }

      // Trigger the start event of the subgraph
      const subgraphStartedEvent = workflowExecutionEntity.buildEvent(buildSubgraphStartedEvent, {
        subgraphId: graphNode.workflowId,
        parentWorkflowId: graphNode.parentWorkflowId!,
        input,
      });
      await emit(this.eventManager, subgraphStartedEvent);
    } else if (isSubgraphEndNode(graphNode)) {
      // Exit the subgraph
      const subgraphContext = workflowExecutionEntity.getCurrentSubgraphContext();
      if (subgraphContext) {
        const output = getSubgraphOutput(workflowExecutionEntity);

        logger.info("Exiting subgraph", {
          executionId,
          subgraphId: subgraphContext.workflowId,
          executionTime: diffTimestamp(subgraphContext.startTime, now()),
        });

        // Trigger the completion event of the subgraph.
        const subgraphCompletedEvent = workflowExecutionEntity.buildEvent(buildSubgraphCompletedEvent, {
          subgraphId: subgraphContext.workflowId,
          output,
          executionTime: diffTimestamp(subgraphContext.startTime, now()),
        });
        await emit(this.eventManager, subgraphCompletedEvent);

        // Pass the original SUBGRAPH node for message context handling
        if (graphNode.originalNode) {
          await exitSubgraph(workflowExecutionEntity, graphNode.originalNode);
        }
      }
    }
  }

  /**
   * Execute node logic
   * @param workflowExecutionContext Workflow execution context
   * @param node Node definition
   * @param abortSignal Optional abort signal for interruption handling
   * @returns Node execution result
   */
  private async executeNodeLogic(
    workflowExecutionEntity: WorkflowExecutionEntity,
    node: WorkflowNode | RuntimeNode,
    _abortSignal?: AbortSignal,
  ): Promise<NodeExecutionResult> {
    const startTime = now();

    // 1. Execute using the Node Handler function (the configuration has been statically verified during workflow registration).
    const handler = getNodeHandler(node.type);

    // 2. Use the factory to create the processor context.
    const handlerContext = this.handlerContextFactory.createHandlerContext(node as RuntimeNode, workflowExecutionEntity);

    // 3. Execute the processor (signal will be used by specific handlers if needed)
    const output = await handler(this.globalContext, workflowExecutionEntity, node as RuntimeNode, handlerContext);

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
      step: workflowExecutionEntity.getNodeResults().length + 1,
      startTime,
      endTime,
      executionTime: diffTimestamp(startTime, endTime),
    };
  }
}
