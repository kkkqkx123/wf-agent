/**
 * Thread Lifecycle Coordinator
 *
 * Responsibilities:
 * - Coordinate the full lifecycle management of threads
 * - Schedule complex operations such as thread creation, execution, suspension, resumption, and termination
 * - Handle multi-step processes and event waiting logic
 *
 * Design Principles:
 * - Stateless design: Does not hold any instance variables
 * - Dependency injection: Receives dependencies through the constructor
 * - Process orchestration: Manages complex multi-step operations and event synchronization
 * - Delegation pattern: Uses the ThreadStateTransitor for atomic state operations
 *
 * Call Path:
 * - External calls: ThreadExecutorAPI → ThreadLifecycleCoordinator
 * - Trigger handling functions should call the Coordinator, not the Manager
 * - The Manager is only used internally as implementation detail for the Coordinator
 */

import { ThreadContextNotFoundError, RuntimeValidationError } from "@wf-agent/types";
import type { ThreadOptions, ThreadResult } from "@wf-agent/types";
import { ThreadStatus } from "@wf-agent/types";
import { ThreadBuilder } from "../factories/thread-builder.js";
import { ThreadExecutor } from "../executors/thread-executor.js";
import { ThreadStateTransitor } from "./thread-state-transitor.js";
import type { ThreadRegistry } from "../../stores/thread-registry.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { AgentLoopRegistry } from "../../../agent/loop/agent-loop-registry.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ThreadLifecycleCoordinator" });

/**
 * Thread Lifecycle Coordinator
 *
 * Responsible for high-level process orchestration and coordination, organizing multiple components to complete complex Thread lifecycle operations.
 */
export class ThreadLifecycleCoordinator {
  constructor(
    private readonly threadRegistry: ThreadRegistry,
    private readonly threadStateTransitor: ThreadStateTransitor,
    private readonly threadBuilder: ThreadBuilder,
    private readonly threadExecutor: ThreadExecutor,
  ) {}

  /**
   * Execute Thread
   *
   * @param workflowId: Workflow ID
   * @param options: Execution options
   * @returns: Execution result
   */
  async execute(workflowId: string, options: ThreadOptions = {}): Promise<ThreadResult> {
    // Step 1: Construct the ThreadEntity
    const { threadEntity } = await this.threadBuilder.build(workflowId, options);

    // Step 2: Register ThreadEntity
    this.threadRegistry.register(threadEntity);

    // Step 3: Start the Thread
    await this.threadStateTransitor.startThread(threadEntity);

    // Step 4: Execute the Thread
    const result = await this.threadExecutor.executeThread(threadEntity);

    // Step 5: Update the Thread status based on the execution results.
    const status = result.metadata?.status;
    const isSuccess = status === "COMPLETED";

    if (isSuccess) {
      await this.threadStateTransitor.completeThread(threadEntity, result);
    } else {
      // Get the first error from the errors array
      const errors = threadEntity.getErrors();
      const lastError =
        errors.length > 0 ? (errors[errors.length - 1] as Error) : new Error("Execution failed");
      await this.threadStateTransitor.failThread(threadEntity, lastError);
    }

    return result;
  }

  /**
   * Pause Thread Execution
   *
   * Process:
   * 1. Obtain the Thread context.
   * 2. Set the pause flag.
   * 3. Trigger the AbortController to interrupt any ongoing asynchronous operations.
   * 4. Update the Thread status.
   *
   * Note:
   * - There is no longer any need to wait for the executor's response; the executor will detect the pause flag at safe points and handle it accordingly.
   * - The AbortController will interrupt any ongoing LLM (Large Language Model) calls and tool executions.
   * - The executor will trigger the THREAD_PAUSED event.
   *
   * @param threadId: Thread ID
   * @throws NotFoundError: The ThreadContext does not exist.
   */
  async pauseThread(threadId: string): Promise<void> {
    const threadEntity = this.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found`, threadId);
    }

    // 1. Request to pause (InterruptionState will automatically trigger the AbortController)
    threadEntity.interrupt("PAUSE");

    // 2. Fully delegate the state transition and event triggering to the Transitor.
    await this.threadStateTransitor.pauseThread(threadEntity);
  }

  /**
   * Recover Thread Execution
   *
   * Process:
   * 1. Obtain the Thread context.
   * 2. Update the Thread status to RUNNING.
   * 3. Clear the pause flag.
   * 4. Continue executing the Thread.
   *
   * @param threadId: Thread ID
   * @returns: Execution result
   * @throws: NotFoundError: The ThreadContext does not exist.
   */
  async resumeThread(threadId: string): Promise<ThreadResult> {
    const threadEntity = this.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found`, threadId);
    }

    // 1. Fully delegate the state transition and event triggering to the Manager.
    await this.threadStateTransitor.resumeThread(threadEntity);

    // 2. Reset the interrupt status (including the AbortController)
    threadEntity.resetInterrupt();

    // 3. Continue execution
    return await this.threadExecutor.executeThread(threadEntity);
  }

  /**
   * Stop Thread Execution
   *
   * Process:
   * 1. Obtain the Thread context.
   * 2. Set the stop flag.
   * 3. Trigger the AbortController to interrupt any ongoing asynchronous operations.
   * 4. Update the Thread status to CANCELLED.
   * 5. Cancel any child Threads recursively.
   * 6. Cleanup child AgentLoops.
   *
   * Note:
   * - There is no need to wait for the executor's response; the executor will detect the stop flag at safe points and handle it accordingly.
   * - The AbortController will interrupt any ongoing LLM (Large Language Model) calls and tool executions.
   * - The executor will trigger the THREAD_CANCELLED event.
   *
   * @param threadId: Thread ID
   * @throws NotFoundError: The ThreadContext does not exist.
   */
  async stopThread(threadId: string): Promise<void> {
    const threadEntity = this.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found`, threadId);
    }

    // 1. Request to stop (InterruptionState will automatically trigger the AbortController)
    threadEntity.interrupt("STOP");

    // 2. Fully delegate the state transitions and event triggering to the Manager.
    await this.threadStateTransitor.cancelThread(threadEntity, "user_requested");

    // 3. Cascading cancellation of child Threads
    await this.threadStateTransitor.cascadeCancel(threadId);

    // 4. Cleanup child AgentLoops
    await this.cleanupChildAgentLoops(threadId);
  }

  /**
   * Cleanup child AgentLoops associated with a thread
   * @param threadId Thread ID
   */
  private async cleanupChildAgentLoops(threadId: string): Promise<void> {
    try {
      const container = getContainer();
      const agentLoopRegistry = container.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry;

      if (agentLoopRegistry) {
        const cleanedCount = agentLoopRegistry.cleanupByParentThreadId(threadId);
        if (cleanedCount > 0) {
          logger.info("Cleaned up child AgentLoops", {
            threadId,
            cleanedCount,
          });
        }
      }
    } catch (error) {
      // Log error but don't throw - cleanup should not prevent thread stopping
      logger.warn("Failed to cleanup child AgentLoops", {
        threadId,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Forcibly set the thread status (directly delegated to ThreadStateTransitor)
   *
   * Note: This is an emergency measure and should typically be used to manage thread status through the normal lifecycle methods (pauseThread/resumeThread/stopThread). Use this method only when those methods are not functioning correctly.
   *
   * @param threadId: Thread ID
   * @param status: New status
   *
   */
  async forceSetThreadStatus(threadId: string, status: ThreadStatus): Promise<void> {
    const threadEntity = this.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found`, threadId);
    }

    switch (status) {
      case "PAUSED":
        await this.threadStateTransitor.pauseThread(threadEntity);
        break;
      case "RUNNING":
        await this.threadStateTransitor.resumeThread(threadEntity);
        break;
      case "CANCELLED":
        await this.threadStateTransitor.cancelThread(threadEntity, "forced");
        break;
      default:
        throw new RuntimeValidationError(`Unsupported status for forceSetThreadStatus: ${status}`, {
          operation: "forceSetThreadStatus",
          field: "status",
          value: status,
        });
    }
  }

  /**
   * Forcibly pause a thread (delegated to ThreadStateTransitor)
   * @param threadId: Thread ID
   */
  async forcePauseThread(threadId: string): Promise<void> {
    await this.forceSetThreadStatus(threadId, "PAUSED");
  }

  /**
   * Forcibly cancel a thread (delegated to ThreadStateTransitor)
   * @param threadId: Thread ID
   * @param reason: Reason for cancellation
   */
  async forceCancelThread(threadId: string, reason?: string): Promise<void> {
    const threadEntity = this.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found`, threadId);
    }

    await this.threadStateTransitor.cancelThread(threadEntity, reason || "forced_cancel");
  }
}
