/**
 * ExecuteThreadStreamCommand - Execute Thread Stream Command
 *
 * Responsibilities:
 * - Encapsulates thread streaming execution as Command pattern
 * - Provides unified API layer interface
 * - Supports streaming Thread execution via event system
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for APIDependencyManager
 * - Returns AsyncGenerator for streaming processing
 *
 * Streaming Event Architecture:
 * - Yields BaseEvent types from EventRegistry
 * - Thread lifecycle events: THREAD_STARTED, THREAD_COMPLETED, etc.
 * - Node events: NODE_STARTED, NODE_COMPLETED, etc.
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { ThreadOptions, BaseEvent } from "@wf-agent/types";
import { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Execute thread stream command parameters
 */
export interface ExecuteThreadStreamParams {
  /** Workflow ID (required) */
  workflowId: string;
  /** Execution options */
  options?: ThreadOptions;
}

/**
 * Thread stream event - union of all relevant events during thread execution
 */
export type ThreadStreamEvent = BaseEvent;

/**
 * Execute Thread Stream Command
 *
 * Workflow:
 * 1. Validate parameters (workflowId is required)
 * 2. Build ThreadEntity using ThreadBuilder
 * 3. Register ThreadEntity
 * 4. Execute thread while yielding events
 * 5. Return final result
 *
 * The stream yields events from the EventRegistry during execution,
 * allowing callers to process events in real-time.
 */
export class ExecuteThreadStreamCommand extends BaseCommand<AsyncGenerator<ThreadStreamEvent>> {
  constructor(
    private readonly params: ExecuteThreadStreamParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<AsyncGenerator<ThreadStreamEvent>> {
    return this.executeStream();
  }

  /**
   * Execute thread and yield events
   */
  private async *executeStream(): AsyncGenerator<ThreadStreamEvent> {
    const lifecycleCoordinator = this.dependencies.getThreadLifecycleCoordinator();
    const threadRegistry = this.dependencies.getThreadRegistry();
    const eventManager = this.dependencies.getEventManager();

    const threadBuilder = (await this.getThreadBuilder()) as {
      build: (
        workflowId: string,
        options: ThreadOptions,
      ) => Promise<import("../../../../graph/entities/thread-entity.js").ThreadEntity>;
    };

    const threadEntity = await threadBuilder.build(
      this.params.workflowId,
      this.params.options || {},
    );
    const threadId = threadEntity.id;

    threadRegistry.register(threadEntity);

    const eventQueue: ThreadStreamEvent[] = [];
    let resolveEvent: ((value: IteratorResult<ThreadStreamEvent>) => void) | null = null;
    let executionComplete = false;
    let finalResult: unknown;

    const eventListener = (event: BaseEvent) => {
      if (event.threadId === threadId) {
        eventQueue.push(event);
        if (resolveEvent) {
          const nextEvent = eventQueue.shift()!;
          resolveEvent({ value: nextEvent, done: false });
          resolveEvent = null;
        }
      }
    };

    const eventTypes: Array<BaseEvent["type"]> = [
      "THREAD_STARTED",
      "THREAD_COMPLETED",
      "THREAD_FAILED",
      "THREAD_PAUSED",
      "THREAD_RESUMED",
      "THREAD_CANCELLED",
      "NODE_STARTED",
      "NODE_COMPLETED",
      "NODE_FAILED",
      "TOOL_CALL_STARTED",
      "TOOL_CALL_COMPLETED",
      "TOOL_CALL_FAILED",
      "ERROR",
    ];

    const unsubscribers: Array<() => void> = [];
    for (const eventType of eventTypes) {
      const unsubscribe = eventManager.on(eventType, eventListener);
      unsubscribers.push(unsubscribe);
    }

    const executionPromise = lifecycleCoordinator
      .execute(this.params.workflowId, this.params.options || {})
      .then(result => {
        finalResult = result;
        executionComplete = true;
      })
      .catch(error => {
        executionComplete = true;
        throw error;
      });

    try {
      while (!executionComplete || eventQueue.length > 0) {
        if (eventQueue.length > 0) {
          yield eventQueue.shift()!;
        } else if (!executionComplete) {
          await Promise.race([
            executionPromise,
            new Promise<IteratorResult<ThreadStreamEvent>>(resolve => {
              resolveEvent = resolve;
            }).then(result => {
              if (result.done === false) {
                return result;
              }
              return null;
            }),
          ]).then(result => {
            if (result && typeof result === "object" && "value" in result) {
              return result as IteratorResult<ThreadStreamEvent>;
            }
            return null;
          });
        }
      }
    } finally {
      for (const unsubscribe of unsubscribers) {
        unsubscribe();
      }
    }

    await executionPromise;
  }

  /**
   * Get ThreadBuilder instance
   */
  private async getThreadBuilder() {
    const container = await import("../../../../core/di/index.js").then(m => m.getContainer());
    const Identifiers = await import("../../../../core/di/service-identifiers.js");
    return container.get(Identifiers.ThreadBuilder);
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.workflowId || this.params.workflowId.trim().length === 0) {
      errors.push("workflowId must be provided");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
