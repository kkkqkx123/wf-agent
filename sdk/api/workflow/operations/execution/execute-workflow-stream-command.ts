/**
 * ExecuteWorkflowStreamCommand - Execute Workflow Stream Command
 *
 * Responsibilities:
 * - Encapsulates workflow streaming execution as Command pattern
 * - Provides unified API layer interface
 * - Supports streaming workflow execution via event system
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for APIDependencyManager
 * - Returns AsyncGenerator for streaming processing
 *
 * Streaming Event Architecture:
 * - Yields BaseEvent types from EventRegistry
 * - Workflow lifecycle events: WORKFLOW_STARTED, WORKFLOW_COMPLETED, etc.
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
 * Execute workflow stream command parameters
 */
export interface ExecuteWorkflowStreamParams {
  /** Workflow ID (required) */
  workflowId: string;
  /** Execution options */
  options?: ThreadOptions;
}

/**
 * Workflow stream event - union of all relevant events during workflow execution
 */
export type WorkflowStreamEvent = BaseEvent;

/**
 * Execute Workflow Stream Command
 *
 * Workflow:
 * 1. Validate parameters (workflowId is required)
 * 2. Build WorkflowExecutionEntity using WorkflowExecutionBuilder
 * 3. Register WorkflowExecutionEntity
 * 4. Execute workflow while yielding events
 * 5. Return final result
 *
 * The stream yields events from the EventRegistry during execution,
 * allowing callers to process events in real-time.
 */
export class ExecuteWorkflowStreamCommand extends BaseCommand<AsyncGenerator<WorkflowStreamEvent>> {
  constructor(
    private readonly params: ExecuteWorkflowStreamParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<AsyncGenerator<WorkflowStreamEvent>> {
    return this.executeStream();
  }

  /**
   * Execute workflow and yield events
   */
  private async *executeStream(): AsyncGenerator<WorkflowStreamEvent> {
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();
    const workflowExecutionRegistry = this.dependencies.getWorkflowExecutionRegistry();
    const eventManager = this.dependencies.getEventManager();

    const workflowExecutionBuilder = (await this.getWorkflowExecutionBuilder()) as {
      build: (
        workflowId: string,
        options: ThreadOptions,
      ) => Promise<import("../../../../workflow/entities/workflow-execution-entity.js").WorkflowExecutionEntity>;
    };

    const executionEntity = await workflowExecutionBuilder.build(
      this.params.workflowId,
      this.params.options || {},
    );
    const executionId = executionEntity.id;

    workflowExecutionRegistry.register(executionEntity);

    const eventQueue: WorkflowStreamEvent[] = [];
    let resolveEvent: ((value: IteratorResult<WorkflowStreamEvent>) => void) | null = null;
    let executionComplete = false;

    const eventListener = (event: BaseEvent) => {
      if (event.threadId === executionId) {
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
      .then(() => {
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
            new Promise<IteratorResult<WorkflowStreamEvent>>(resolve => {
              resolveEvent = resolve;
            }).then(result => {
              if (result.done === false) {
                return result;
              }
              return null;
            }),
          ]).then(result => {
            if (result && typeof result === "object" && "value" in result) {
              return result as IteratorResult<WorkflowStreamEvent>;
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
   * Get WorkflowExecutionBuilder instance
   */
  private async getWorkflowExecutionBuilder() {
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
