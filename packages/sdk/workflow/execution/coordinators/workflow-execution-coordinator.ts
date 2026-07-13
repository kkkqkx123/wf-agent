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
  // [Issue 4 Fix] Track workflow-level retry statistics
  private _workflowRetryCount: number = 0;
  private _workflowRetryDelays: number[] = [];

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

    // Get workflow-level failure strategy from entity
    const onFailure = this.workflowExecutionEntity.getWorkflowOnFailure();
    const maxRetries = this.workflowExecutionEntity.getWorkflowMaxRetries();
    const retryDelayMs = this.workflowExecutionEntity.getWorkflowRetryDelayMs();
    const exponentialBackoff = this.workflowExecutionEntity.getWorkflowExponentialBackoff();

    // Wrap execution with workflow-level failure policy
    const maxAttempts = onFailure === "retry" ? (maxRetries + 1) : 1;
    let lastResult: WorkflowExecutionResult | undefined;
    this._workflowRetryCount = 0;
    this._workflowRetryDelays = [];

    for (let attemptCount = 0; attemptCount < maxAttempts; attemptCount++) {
      // For retry attempts after the first, reset entity state and start fresh
      if (attemptCount > 0) {
        logger.info("Retrying workflow execution from beginning", {
          executionId: this.workflowExecutionEntity.id,
          attemptCount,
          maxAttempts,
        });
        // Reset state to RUNNING, clear node results and errors
        this.workflowExecutionEntity.state.reset();
        this.workflowExecutionEntity.state.start();
        this.workflowExecutionEntity.clearNodeResults();
        // Reset to the first real node after START
        const startNodeId = this.navigator.getGraph().startNodeId;
        if (startNodeId) {
          const firstNode = this.navigator.getNextNode(startNodeId);
          if (firstNode && firstNode.nextNodeId) {
            this.workflowExecutionEntity.setCurrentNodeId(firstNode.nextNodeId);
          }
        }
      }

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

          // Update the current node ID based on result
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
            // Node execution failed
            const errorMessage = nodeResult.error instanceof Error ? nodeResult.error.message : String(nodeResult.error) || "Unknown error";

            if (onFailure === "continue") {
              // [Issue 1 Fix] Continue strategy: treat failed node as SKIPPED and continue
              logger.warn("Node execution failed - continuing workflow (onFailure=continue)", {
                executionId: this.workflowExecutionEntity.id,
                nodeId: currentNodeId,
                error: errorMessage,
              });
              // Advance to next node as if the node was skipped
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
              // "fail" or "retry" strategy: fail the workflow and stop
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
            }
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

        // Build execution result
        const status = this.workflowExecutionEntity.getStatus();
        if (status === "FAILED" || status === "CANCELLED" || status === "CREATED") {
          return this.buildSuccessResult();
        }

        // For "continue" strategy with fallbackOutput: if the workflow is in a failed-like state
        // after processing all reachable nodes, still produce a result
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

      lastResult = result.result;

      // Check if execution was successful or we're out of retry attempts
      const execStatus = lastResult.metadata?.status;
      if (execStatus === "COMPLETED" || execStatus === "PAUSED" || execStatus === "CANCELLED" || execStatus === "STOPPED") {
        return lastResult;
      }

      // If failed and onFailure is "retry", calculate delay and retry
      if (onFailure === "retry" && attemptCount < maxAttempts - 1) {
        const delayMs = exponentialBackoff
          ? Math.min(retryDelayMs * Math.pow(2, attemptCount), 60000)
          : retryDelayMs;

        this._workflowRetryDelays.push(delayMs);
        this._workflowRetryCount++;

        logger.info("Workflow execution failed, retrying", {
          executionId: this.workflowExecutionEntity.id,
          attemptCount,
          delayMs,
          nextAttempt: attemptCount + 2,
          maxAttempts,
        });

        await new Promise(resolve => setTimeout(resolve, delayMs));
      }
    }

    // If onFailure is "continue" with fallbackOutput, produce a successful result with fallback
    if (onFailure === "continue") {
      const fallbackOutput = this.workflowExecutionEntity.getWorkflowFallbackOutput();
      if (fallbackOutput) {
        logger.info("Workflow execution failed, using fallback output (onFailure=continue)", {
          executionId: this.workflowExecutionEntity.id,
        });
        return {
          executionId: this.workflowExecutionEntity.id,
          output: fallbackOutput.output ?? {},
          executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() ?? Date.now()),
          nodeResults: this.workflowExecutionEntity.getNodeResults(),
          metadata: {
            status: "COMPLETED",
            startTime: this.workflowExecutionEntity.getStartTime() ?? Date.now(),
            endTime: Date.now(),
            executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() ?? Date.now()),
            nodeCount: this.workflowExecutionEntity.getNodeResults().length,
            errorCount: this.workflowExecutionEntity.getErrors().length,
          },
        };
      }
    }

    // Return the last result (failed status)
    return lastResult!;
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
      this.workflowExecutionEntity.state.pause(currentNodeId);
    } else {
      this.workflowExecutionEntity.state.cancel(currentNodeId);
    }

    // [Problem #4 Fix] Collect error details from node results
    const nodeResults = this.workflowExecutionEntity.getNodeResults();
    const errors: WorkflowExecutionResult["errors"] = [];
    for (const nr of nodeResults) {
      if (nr.status === "FAILED" && nr.error) {
        errors.push({
          nodeId: nr.nodeId,
          message: nr.error instanceof Error ? nr.error.message : String(nr.error),
          type: nr.error instanceof Error ? nr.error.name : "unknown",
        });
      }
    }

    // [P10 Fix] Record failed node errors into state errorRecords for checkpoint consistency
    // This ensures the error chain is preserved even if the execution is interrupted.
    for (const err of errors) {
      if (err.nodeId) {
        this.workflowExecutionEntity.state.recordError({
          id: `error:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`,
          timestamp: Date.now(),
          message: err.message,
          errorType: "execution_error",
          severity: "error",
          nodeId: err.nodeId,
          context: { operation: "workflow_interruption" },
          isRecoverable: false,
        });
      }
    }

    // Build interrupted result
    return {
      executionId: this.workflowExecutionEntity.id,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() || Date.now()),
      nodeResults,
      metadata: {
        status: type === "PAUSE" ? "PAUSED" : "CANCELLED",
        startTime: this.workflowExecutionEntity.getStartTime() || Date.now(),
        endTime: Date.now(),
        executionTime: Date.now() - (this.workflowExecutionEntity.getStartTime() || Date.now()),
        nodeCount: nodeResults.length,
        errorCount: this.workflowExecutionEntity.getErrors().length,
        interruptionType: type,
        interruptedAtNodeId: currentNodeId,
      },
      // [Problem #4 Fix] Include structured error details
      errors: errors.length > 0 ? errors : undefined,
    };
  }

  /**
   * Build successful execution result
   * @returns Workflow execution result with success status
   */
  private buildSuccessResult(): WorkflowExecutionResult {
    const endTime = this.workflowExecutionEntity.getEndTime() || Date.now();
    const startTime = this.workflowExecutionEntity.getStartTime() || Date.now();
    const rawErrors = this.workflowExecutionEntity.getErrors();

    // [Problem #4 Fix] Collect error details from node results and global errors
    const errors: WorkflowExecutionResult["errors"] = [];
    const nodeResults = this.workflowExecutionEntity.getNodeResults();
    for (const nr of nodeResults) {
      if (nr.status === "FAILED" && nr.error) {
        errors.push({
          nodeId: nr.nodeId,
          message: nr.error instanceof Error ? nr.error.message : String(nr.error),
          type: nr.error instanceof Error ? nr.error.name : "unknown",
        });
      }
    }
    for (const raw of rawErrors) {
      errors.push({
        message: typeof raw === "string" ? raw : raw instanceof Error ? raw.message : String(raw),
        type: raw instanceof Error ? raw.name : "string",
      });
    }

    // [Issue 4 Fix] Build error chain from state error records
    const state = this.workflowExecutionEntity.state;
    const errorRecords = typeof state.getErrorRecords === "function" ? state.getErrorRecords() : [];
    const errorChain = errorRecords.length > 0
      ? errorRecords.map(r => ({
          id: r.id,
          message: r.message,
          errorType: r.errorType,
          severity: r.severity,
          nodeId: r.nodeId,
          timestamp: r.timestamp,
        }))
      : undefined;

    // Calculate retry statistics
    const workflowRetryDelayTime = this._workflowRetryDelays.reduce((a, b) => a + b, 0);

    return {
      executionId: this.workflowExecutionEntity.id,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime: endTime - startTime,
      nodeResults,
      metadata: {
        status: this.workflowExecutionEntity.getStatus(),
        startTime,
        endTime,
        executionTime: endTime - startTime,
        nodeCount: nodeResults.length,
        errorCount: rawErrors.length,
      },
      // [Problem #4 Fix] Include structured error details
      errors: errors.length > 0 ? errors : undefined,
      // [Issue 4 Fix] Include retry statistics and error chain
      workflowRetryCount: this._workflowRetryCount,
      workflowRetryDelayTime: workflowRetryDelayTime > 0 ? workflowRetryDelayTime : undefined,
      totalRetryCount: this._workflowRetryCount > 0 ? this._workflowRetryCount : undefined,
      totalRetryDelayTime: workflowRetryDelayTime > 0 ? workflowRetryDelayTime : undefined,
      errorChain,
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
