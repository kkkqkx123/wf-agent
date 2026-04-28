/**
 * ThreadExecutor - Thread Executor
 * Responsible for executing a single ThreadEntity instance and managing the entire lifecycle of the thread.
 * Supports node navigation using the graph navigator.
 *
 * Responsibilities:
 * - Executes a single ThreadEntity.
 * - Coordinates the execution of various components to complete the task.
 *
 * Does not handle:
 * - The creation and registration of threads (handled by ThreadLifecycleCoordinator).
 * - Lifecycle management tasks such as pausing, resuming, and stopping threads (handled by ThreadLifecycleCoordinator).
 * - Variable setting and other management operations (handled by ThreadLifecycleCoordinator).
 * - Details of node execution (handled by NodeExecutionCoordinator).
 * - Error handling (handled by ErrorHandler).
 * - Subgraph processing (handled by SubgraphHandler).
 * - Triggering sub-workflow processing (handled by TriggeredSubworkflowHandler).
 *
 * Design Principles:
 * - Stateless design; all state is managed through ThreadEntity.
 * - Supports pausing/resuming functionality.
 * - Supports interruption control (AbortController).
 * - Consistent architecture with LLMExecutor and ToolCallExecutor.
 */

import type { ThreadResult } from "@wf-agent/types";
import type { ThreadEntity } from "../../entities/thread-entity.js";
import type { GraphRegistry } from "../../stores/graph-registry.js";
import type { ThreadExecutionCoordinator } from "../coordinators/thread-execution-coordinator.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "thread-executor" });

/**
 * Thread Isolation Manager Factory Interface
 */

/**
 * ThreadExecutionCoordinator factory interface
 */
interface ThreadExecutionCoordinatorFactory {
  create(threadEntity: ThreadEntity): ThreadExecutionCoordinator;
}

/**
 * ThreadExecutor dependency configuration
 */
export interface ThreadExecutorDependencies {
  /** Figure Registry */
  graphRegistry: GraphRegistry;
  /** ThreadExecutionCoordinator factory */
  threadExecutionCoordinatorFactory: ThreadExecutionCoordinatorFactory;
}

/**
 * ThreadExecutor - A stateless thread executor
 *
 * Focuses on executing a single ThreadEntity, without being responsible for thread creation, registration, or management.
 * Delegates specific responsibilities to dedicated components through the coordinator pattern.
 *
 * Design principles:
 * - Stateless design: Does not retain any internal state.
 * - All state is passed in through the ThreadEntity parameters.
 * - Dependencies are injected through the constructor (Dependency Inversion).
 * - Keeps a lightweight design, focusing solely on execution coordination.
 * - Does not directly depend on the DI container, which facilitates testing.
 * - Its lifecycle is managed by the DI container.
 */
export class ThreadExecutor {
  private graphRegistry: GraphRegistry;
  private threadExecutionCoordinatorFactory: ThreadExecutionCoordinatorFactory;

  constructor(deps: ThreadExecutorDependencies) {
    this.graphRegistry = deps.graphRegistry;
    this.threadExecutionCoordinatorFactory = deps.threadExecutionCoordinatorFactory;
  }

  /**
   * Execute ThreadEntity
   * @param threadEntity: An instance of ThreadEntity
   * @returns: The execution result
   */
  async executeThread(threadEntity: ThreadEntity): Promise<ThreadResult> {
    const threadId = threadEntity.id;
    const workflowId = threadEntity.getWorkflowId();

    logger.info("Starting thread execution", { threadId, workflowId });

    // Verify the existence of the workflow diagram.
    const preprocessedGraph = this.graphRegistry.get(workflowId);
    if (!preprocessedGraph) {
      throw new Error(`Graph not found for workflow: ${workflowId}`);
    }

    // Create a ThreadExecutionCoordinator using a factory and execute it.
    const threadExecutionCoordinator = this.threadExecutionCoordinatorFactory.create(threadEntity);

    // Execute Thread
    const result = await threadExecutionCoordinator.execute();

    logger.info("Thread execution completed", {
      threadId,
      workflowId,
      nodeCount: result.nodeResults?.length ?? 0,
    });

    return result;
  }
}
