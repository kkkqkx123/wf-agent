/**
 * WorkflowExecutionCoordinator - Workflow Execution Coordinator
 * Coordinates the execution flow of a workflow execution and orchestrates the execution of each component to complete its task.
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionResult, Condition, EvaluationContext } from "@wf-agent/types";
import type { InterruptionState } from "../../../shared/utils/interruption/interruption-state.js";
import type { NodeExecutionCoordinator } from "./node-execution-coordinator.js";
import type { WorkflowNavigator } from "../../builder/workflow-navigator.js";
import { conditionEvaluator } from "../../../services/evaluation/index.js";
import { executeWithInterruptionHandling } from "../../../shared/utils/interruption/index.js";
import {
  toWorkflowInterruptionResult,
  type WorkflowInterruptionCheckResult,
} from "../utils/workflow-interruption-utils.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type { TimeoutHandle } from "../../../shared/types/timeout.js";

const logger = createContextualLogger({ component: "WorkflowExecutionCoordinator" });

/**
 * WorkflowExecutionCoordinator - Workflow Execution Coordinator
 *
 * Responsibilities:
 * - Coordinate the flow of workflow execution
 * - Orchestrate the execution of each component to complete its task
 * - Handle interrupt status
 * - Manage the node execution loop.
 *
 * Design Principles:
 * - Coordination logic: encapsulate complex execution coordination logic
 * - Dependency Injection: Receive dependent coordinators and managers through the constructor.
 * - Flow orchestration: call the components in the right order
 */
export class WorkflowExecutionCoordinator {
  constructor(
    private readonly workflowExecutionEntity: WorkflowExecutionEntity,
    private readonly interruptionManager: InterruptionState,
    private readonly nodeExecutionCoordinator: NodeExecutionCoordinator,
    private readonly navigator: WorkflowNavigator,
  ) {}

  /**
   * Execute the workflow execution
   * @returns The result of the workflow execution
   */
  async execute(): Promise<WorkflowExecutionResult> {
    const abortSignal = this.interruptionManager.getAbortSignal();

    // Use unified interruption handling wrapper
    const result = await executeWithInterruptionHandling(async _signal => {
      // Execution process orchestration
      while (true) {
        // Get the current node
        const currentNodeId = this.workflowExecutionEntity.getCurrentNodeId();
        if (!currentNodeId) {
          break;
        }

        // Phase 4: Skip already completed nodes (for resume from checkpoint)
        // When resuming from a checkpoint, the currentNodeId may point to a node
        // that was already completed. Skip it and advance to the next node.
        const completedNodeIds = new Set(
          this.workflowExecutionEntity
            .getNodeResults()
            .filter(r => r.status === "COMPLETED")
            .map(r => r.nodeId),
        );
        if (completedNodeIds.has(currentNodeId)) {
          logger.debug("Skipping already completed node during resume", {
            executionId: this.workflowExecutionEntity.id,
            nodeId: currentNodeId,
          });
          // Advance to next node and continue the outer loop
          const outgoingEdges = this.navigator.getGraph().getOutgoingEdges(currentNodeId);
          if (outgoingEdges.length <= 1) {
            const nextNode = this.navigator.getNextNode(currentNodeId);
            if (nextNode && nextNode.nextNodeId) {
              this.workflowExecutionEntity.setCurrentNodeId(nextNode.nextNodeId);
            } else {
              break;
            }
          } else {
            const evalContext = this.buildRoutingEvaluationContext();
            const routingResult = this.navigator.routeNextNode(
              currentNodeId,
              (condition: Condition) => conditionEvaluator.evaluate(condition, evalContext),
            );
            if (routingResult) {
              this.workflowExecutionEntity.setCurrentNodeId(routingResult.selectedNodeId);
            } else {
              break;
            }
          }
          continue; // Re-check the outer loop with the new node
        }

        // Get the node object from the graph (already a RuntimeNode after preprocessing)
        const currentNode = this.navigator.getGraph().getNode(currentNodeId);
        if (!currentNode) {
          break;
        }

        // Phase 5: Node Completion Timeout
        // Register a per-node timeout on the entity's TimeoutManager
        const nodeTimeoutMs = this.workflowExecutionEntity.getNodeTimeout();
        let nodeTimeoutHandle: TimeoutHandle | undefined;
        const nodeAbortController = new AbortController();

        nodeTimeoutHandle = this.workflowExecutionEntity.timeoutManager.register({
          id: `node-${currentNodeId}-${Date.now()}`,
          duration: nodeTimeoutMs,
          onTimeout: () => {
            logger.warn(`Node '${currentNodeId}' timed out after ${nodeTimeoutMs}ms`, {
              executionId: this.workflowExecutionEntity.id,
              nodeId: currentNodeId,
            });
            nodeAbortController.abort(`Node ${currentNodeId} timed out after ${nodeTimeoutMs}ms`);
          },
          tag: "node-execution",
          metadata: {
            nodeId: currentNodeId,
            nodeTimeoutMs,
          },
          interruptionState: this.workflowExecutionEntity.getInterruptionState(),
        });

        // Use only the node timeout signal - the main interruption signal is
        // handled by the executeWithInterruptionHandling wrapper at the outer level.
        // Wall-clock timeout serves as the final circuit breaker, not mixed with node timeout.
        const nodeSignal = nodeAbortController.signal;

        // Execute Node with node signal
        const nodeResult = await this.nodeExecutionCoordinator.executeNode(
          this.workflowExecutionEntity,
          currentNode,
          { abortSignal: nodeSignal },
        );

        // Cancel the node timeout handle
        if (nodeTimeoutHandle) {
          nodeTimeoutHandle.cancel();
        }

        // Update the current node ID
        if (nodeResult.status === "COMPLETED") {
          const outgoingEdges = this.navigator.getGraph().getOutgoingEdges(currentNodeId);
          if (outgoingEdges.length <= 1) {
            // Single outgoing edge: use simple navigation
            const nextNode = this.navigator.getNextNode(currentNodeId);
            if (nextNode && nextNode.nextNodeId) {
              this.workflowExecutionEntity.setCurrentNodeId(nextNode.nextNodeId);
            } else {
              break;
            }
          } else {
            // Multiple outgoing edges: use conditional routing
            const evalContext = this.buildRoutingEvaluationContext();
            const routingResult = this.navigator.routeNextNode(
              currentNodeId,
              (condition: Condition) => conditionEvaluator.evaluate(condition, evalContext),
            );
            if (routingResult) {
              this.workflowExecutionEntity.setCurrentNodeId(routingResult.selectedNodeId);
            } else {
              break;
            }
          }
        } else if (nodeResult.status === "FAILED") {
          // Node execution failed - update workflow status and stop execution
          const errorMessage = nodeResult.error instanceof Error ? nodeResult.error.message : String(nodeResult.error) || "Unknown error";
          logger.error("Node execution failed - stopping workflow", {
            executionId: this.workflowExecutionEntity.id,
            nodeId: currentNodeId,
            nodeStatus: nodeResult.status,
            error: errorMessage,
          });

          // Update workflow status to FAILED
          this.workflowExecutionEntity.state.fail(
            nodeResult.error instanceof Error ? nodeResult.error : new Error(`Node '${currentNodeId}' execution failed`),
          );

          // Stop workflow execution
          break;
        } else if (nodeResult.status === "SKIPPED") {
          // Node was skipped - log and continue to next
          const skipReason = nodeResult.error instanceof Error ? nodeResult.error.message : (typeof nodeResult.error === "string" ? nodeResult.error : "Node was skipped");
          logger.info("Node execution skipped", {
            executionId: this.workflowExecutionEntity.id,
            nodeId: currentNodeId,
            reason: skipReason,
          });

          // Continue to next node after skipped node
          const outgoingEdges = this.navigator.getGraph().getOutgoingEdges(currentNodeId);
          if (outgoingEdges.length <= 1) {
            const nextNode = this.navigator.getNextNode(currentNodeId);
            if (nextNode && nextNode.nextNodeId) {
              this.workflowExecutionEntity.setCurrentNodeId(nextNode.nextNodeId);
            } else {
              break;
            }
          } else {
            const evalContext = this.buildRoutingEvaluationContext();
            const routingResult = this.navigator.routeNextNode(
              currentNodeId,
              (condition: Condition) => conditionEvaluator.evaluate(condition, evalContext),
            );
            if (routingResult) {
              this.workflowExecutionEntity.setCurrentNodeId(routingResult.selectedNodeId);
            } else {
              break;
            }
          }
        } else {
          // Unknown node status - stop execution
          logger.error("Unknown node execution status", {
            executionId: this.workflowExecutionEntity.id,
            nodeId: currentNodeId,
            status: nodeResult.status,
          });

          // Update workflow status to FAILED
          this.workflowExecutionEntity.state.fail(
            new Error(`Node '${currentNodeId}' returned unknown status: ${nodeResult.status}`),
          );

          break;
        }
      }

      // Build successful execution result
      return this.buildSuccessResult();
    }, abortSignal);

    // Handle interruption gracefully if needed
    if (!result.success) {
      const workflowInterruption = toWorkflowInterruptionResult(
        result.interruption,
        this.workflowExecutionEntity.getCurrentNodeId() || "unknown",
      );
      return await this.handleInterruptionGracefully(workflowInterruption);
    }

    return result.result;
  }

  /**
   * Handle interruption gracefully using return-value pattern
   * @param interruption Interruption check result
   * @returns Workflow execution result with interruption status
   */
  private async handleInterruptionGracefully(
    interruption: WorkflowInterruptionCheckResult,
  ): Promise<WorkflowExecutionResult> {
    const type = interruption.type === "paused" ? "PAUSE" : "STOP";
    const currentNodeId = this.workflowExecutionEntity.getCurrentNodeId();

    // Delegate to node coordinator for checkpoint creation and event emission
    if (currentNodeId) {
      await this.nodeExecutionCoordinator.handleInterruption(
        this.workflowExecutionEntity.id,
        currentNodeId,
        type,
      );
    }

    // Update status via state lifecycle method
    if (type === "PAUSE") {
      this.workflowExecutionEntity.state.pause();
    } else {
      this.workflowExecutionEntity.state.cancel();
    }

    // Build interrupted result
    return {
      executionId: this.workflowExecutionEntity.id,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() || Date.now()),
      nodeResults: this.workflowExecutionEntity.getNodeResults(),
      metadata: {
        status: type === "PAUSE" ? "PAUSED" : "CANCELLED",
        startTime: this.workflowExecutionEntity.getStartTime() || Date.now(),
        endTime: Date.now(),
        executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() || Date.now()),
        nodeCount: this.workflowExecutionEntity.getNodeResults().length,
        errorCount: this.workflowExecutionEntity.getErrors().length,
        interruptionType: type,
        interruptedAtNodeId: currentNodeId,
      },
    };
  }

  /**
   * Build successful execution result
   * @returns Workflow execution result with success status
   */
  private buildSuccessResult(): WorkflowExecutionResult {
    const endTime = this.workflowExecutionEntity.getEndTime() || Date.now();
    const startTime = this.workflowExecutionEntity.getStartTime() || Date.now();

    return {
      executionId: this.workflowExecutionEntity.id,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime: endTime - startTime,
      nodeResults: this.workflowExecutionEntity.getNodeResults(),
      metadata: {
        status: this.workflowExecutionEntity.getStatus(),
        startTime,
        endTime,
        executionTime: endTime - startTime,
        nodeCount: this.workflowExecutionEntity.getNodeResults().length,
        errorCount: this.workflowExecutionEntity.getErrors().length,
      },
    };
  }

  /**
   * Build routing evaluation context for conditional edge evaluation
   * @returns EvaluationContext with workflow input, output, and variables
   */
  private buildRoutingEvaluationContext(): EvaluationContext {
    return {
      variables: this.workflowExecutionEntity.getAllVariables(),
      input: this.workflowExecutionEntity.getInput(),
      output: this.workflowExecutionEntity.getOutput(),
    };
  }

  /**
   * Pause workflow execution
   */
  pause(): void {
    this.interruptionManager.requestPause();
  }

  /**
   * Resume workflow execution
   */
  resume(): void {
    this.interruptionManager.resume();
  }

  /**
   * Stop the execution of workflow.
   */
  stop(): void {
    this.interruptionManager.requestStop();
  }

  /**
   * Get a WorkflowExecutionEntity instance
   * @returns A WorkflowExecutionEntity instance
   */
  getWorkflowExecutionEntity(): WorkflowExecutionEntity {
    return this.workflowExecutionEntity;
  }
}
