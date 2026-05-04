/**
 * WorkflowExecutionCoordinator - Workflow Execution Coordinator
 * Coordinates the execution flow of a workflow execution and orchestrates the execution of each component to complete its task.
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowExecutionResult, Node as WorkflowNode } from "@wf-agent/types";
import type { VariableCoordinator } from "./variable-coordinator.js";
import type { TriggerCoordinator } from "./trigger-coordinator.js";
import type { InterruptionState } from "../../../core/types/interruption-state.js";
import type { ToolVisibilityCoordinator } from "./tool-visibility-coordinator.js";
import type { NodeExecutionCoordinator } from "./node-execution-coordinator.js";
import type { WorkflowNavigator } from "../../builder/workflow-navigator.js";
import {
  checkInterruption,
  shouldContinue,
  getInterruptionDescription,
} from "@wf-agent/common-utils";
import type { InterruptionCheckResult } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "workflow-execution-coordinator" });

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
    private readonly variableCoordinator: VariableCoordinator,
    private readonly triggerCoordinator: TriggerCoordinator,
    private readonly interruptionManager: InterruptionState,
    private readonly toolVisibilityCoordinator: ToolVisibilityCoordinator,
    private readonly nodeExecutionCoordinator: NodeExecutionCoordinator,
    private readonly navigator: WorkflowNavigator,
  ) {}

  /**
   * Execute the workflow execution
   * @returns The result of the workflow execution
   */
  async execute(): Promise<WorkflowExecutionResult> {
    const executionId = this.workflowExecutionEntity.id;
    const startTime = this.workflowExecutionEntity.getStartTime();

    // Execution process orchestration
    while (true) {
      // ✅ Use return-value pattern instead of throwing exceptions
      const interruption = checkInterruption(this.interruptionManager.getAbortSignal());

      if (!shouldContinue(interruption)) {
        logger.info("Workflow execution interrupted", {
          executionId,
          interruptionType: interruption.type,
          currentNodeId: this.workflowExecutionEntity.getCurrentNodeId(),
        });

        // Handle interruption gracefully
        return await this.handleInterruptionGracefully(interruption);
      }

      // Get the current node
      const currentNodeId = this.workflowExecutionEntity.getCurrentNodeId();
      if (!currentNodeId) {
        break;
      }

      // Get the node object
      const graphNode = this.navigator.getGraph().getNode(currentNodeId);
      if (!graphNode) {
        break;
      }

      // Use `originalNode` or create a new `Node`.
      const currentNode =
        graphNode.originalNode ||
        ({
          id: graphNode.id,
          type: graphNode.type,
          name: graphNode.name,
          config: {},
          outgoingEdgeIds: [],
          incomingEdgeIds: [],
        } as WorkflowNode);

      // Execute Node
      const result = await this.nodeExecutionCoordinator.executeNode(
        this.workflowExecutionEntity,
        currentNode,
      );

      // Update node results
      this.workflowExecutionEntity.addNodeResult(result);

      // Update the current node ID - Use the navigator to get the next node
      if (result.status === "COMPLETED") {
        const nextNode = this.navigator.getNextNode(currentNodeId);
        if (nextNode && nextNode.nextNodeId) {
          this.workflowExecutionEntity.setCurrentNodeId(nextNode.nextNodeId);
        } else {
          break;
        }
      } else {
        break;
      }
    }

    // Build successful execution result
    return this.buildSuccessResult();
  }

  /**
   * Handle interruption gracefully using return-value pattern
   * @param interruption Interruption check result
   * @returns Workflow execution result with interruption status
   */
  private async handleInterruptionGracefully(
    interruption: InterruptionCheckResult,
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

    // Update status
    this.workflowExecutionEntity.setStatus(type === "PAUSE" ? "PAUSED" : "CANCELLED");

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
