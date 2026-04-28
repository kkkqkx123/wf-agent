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
import { WorkflowExecutionInterruptedException } from "@wf-agent/types";

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
      // Check the interrupt status.
      if (this.interruptionManager.shouldPause()) {
        throw new WorkflowExecutionInterruptedException(
          "Workflow execution paused",
          "PAUSE",
          executionId,
          this.workflowExecutionEntity.getCurrentNodeId(),
        );
      }

      if (this.interruptionManager.shouldStop()) {
        throw new WorkflowExecutionInterruptedException(
          "Workflow execution stopped",
          "STOP",
          executionId,
          this.workflowExecutionEntity.getCurrentNodeId(),
        );
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

    // Build the execution results
    const endTime = this.workflowExecutionEntity.getEndTime() || Date.now();
    const executionTime = endTime - (startTime || Date.now());

    return {
      executionId,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime,
      nodeResults: this.workflowExecutionEntity.getNodeResults(),
      metadata: {
        status: this.workflowExecutionEntity.getStatus(),
        startTime: startTime || Date.now(),
        endTime,
        executionTime,
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
