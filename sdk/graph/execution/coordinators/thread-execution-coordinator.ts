/**
 * ThreadExecutionCoordinator - Thread Execution Coordinator
 * Coordinates the execution flow of a Thread and orchestrates the execution of each component to complete its task.
 */

import type { ThreadEntity } from "../../entities/thread-entity.js";
import type { ThreadResult, Node as WorkflowNode } from "@wf-agent/types";
import type { VariableCoordinator } from "./variable-coordinator.js";
import type { TriggerCoordinator } from "./trigger-coordinator.js";
import type { InterruptionState } from "../../../core/types/interruption-state.js";
import type { ToolVisibilityCoordinator } from "./tool-visibility-coordinator.js";
import type { NodeExecutionCoordinator } from "./node-execution-coordinator.js";
import type { GraphNavigator } from "../../graph-builder/graph-navigator.js";
import { ThreadInterruptedException } from "@wf-agent/types";

/**
 * ThreadExecutionCoordinator - Thread Execution Coordinator
 *
 * Responsibilities:
 * - Coordinate the flow of Thread execution
 * - Orchestrate the execution of each component to complete its task
 * - Handle interrupt status
 * - Manage the node execution loop.
 *
 * Design Principles:
 * - Coordination logic: encapsulate complex execution coordination logic
 * - Dependency Injection: Receive dependent coordinators and managers through the constructor.
 * - Flow orchestration: call the components in the right order
 */
export class ThreadExecutionCoordinator {
  constructor(
    private readonly threadEntity: ThreadEntity,
    private readonly variableCoordinator: VariableCoordinator,
    private readonly triggerCoordinator: TriggerCoordinator,
    private readonly interruptionManager: InterruptionState,
    private readonly toolVisibilityCoordinator: ToolVisibilityCoordinator,
    private readonly nodeExecutionCoordinator: NodeExecutionCoordinator,
    private readonly navigator: GraphNavigator,
  ) {}

  /**
   * Execute the Thread
   * @returns The result of the Thread execution
   */
  async execute(): Promise<ThreadResult> {
    const threadId = this.threadEntity.id;
    const startTime = this.threadEntity.getStartTime();

    // Execution process orchestration
    while (true) {
      // Check the interrupt status.
      if (this.interruptionManager.shouldPause()) {
        throw new ThreadInterruptedException(
          "Thread execution paused",
          "PAUSE",
          threadId,
          this.threadEntity.getCurrentNodeId(),
        );
      }

      if (this.interruptionManager.shouldStop()) {
        throw new ThreadInterruptedException(
          "Thread execution stopped",
          "STOP",
          threadId,
          this.threadEntity.getCurrentNodeId(),
        );
      }

      // Get the current node
      const currentNodeId = this.threadEntity.getCurrentNodeId();
      if (!currentNodeId) {
        break;
      }

      // Get the node object
      const graphNode = this.navigator.getGraph().getNode(currentNodeId);
      if (!graphNode) {
        break;
      }

      // Use `originalNode` or create a new `Node`.
      // Use type assertions, since graphNode.type is dynamic
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
        this.threadEntity,
        currentNode,
      );

      // Update node results
      this.threadEntity.addNodeResult(result);

      // Update the current node ID - Use the navigator to get the next node
      if (result.status === "COMPLETED") {
        const nextNode = this.navigator.getNextNode(currentNodeId);
        if (nextNode && nextNode.nextNodeId) {
          this.threadEntity.setCurrentNodeId(nextNode.nextNodeId);
        } else {
          break;
        }
      } else {
        break;
      }
    }

    // Build the execution results
    const endTime = this.threadEntity.getEndTime() || Date.now();
    const executionTime = endTime - (startTime || Date.now());

    return {
      threadId,
      output: this.threadEntity.getOutput(),
      executionTime,
      nodeResults: this.threadEntity.getNodeResults(),
      metadata: {
        status: this.threadEntity.getStatus(),
        startTime: startTime || Date.now(),
        endTime,
        executionTime,
        nodeCount: this.threadEntity.getNodeResults().length,
        errorCount: this.threadEntity.getErrors().length,
      },
    };
  }

  /**
   * Pause Thread execution
   */
  pause(): void {
    this.interruptionManager.requestPause();
  }

  /**
   * Recover Thread execution
   */
  resume(): void {
    this.interruptionManager.resume();
  }

  /**
   * Stop the execution of Thread.
   */
  stop(): void {
    this.interruptionManager.requestStop();
  }

  /**
   * Get a ThreadEntity instance
   * @returns A ThreadEntity instance
   */
  getThreadEntity(): ThreadEntity {
    return this.threadEntity;
  }
}
