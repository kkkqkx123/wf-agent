/**
 * ExecuteWorkflowStreamCommand - Execute Workflow Stream Command
 *
 * Category: Execution (Streaming)
 * Executes workflow while streaming events in real-time
 */

import {
  StreamingCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateWorkflowExecutionParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { WorkflowExecutionOptions, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { WorkflowExecutionBuildResult } from "../../../../workflow/execution/factories/workflow-execution-builder.js";
import * as ServiceIdentifiers from "../../../../di/service-identifiers.js";

/**
 * Execute workflow stream command parameters
 */
export interface ExecuteWorkflowStreamParams {
  /** Workflow ID (required) */
  workflowId: string;
  /** Execution options */
  options?: WorkflowExecutionOptions;
}

/**
 * Execute Workflow Stream Command
 * Executes workflow and yields events as they occur
 */
export class ExecuteWorkflowStreamCommand extends StreamingCommand<AsyncGenerator<BaseEvent>> {
  constructor(
    private readonly params: ExecuteWorkflowStreamParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ExecuteWorkflowStreamCommand",
      description: "Execute a workflow and stream events in real-time",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<AsyncGenerator<BaseEvent>> {
    return this.executeStream();
  }

  /**
   * Execute workflow and yield events
   */
  private async *executeStream(): AsyncGenerator<BaseEvent> {
    const lifecycleCoordinator = this.dependencies.getWorkflowLifecycleCoordinator();
    const workflowExecutionRegistry = this.dependencies.getWorkflowExecutionRegistry();
    const eventManager = this.dependencies.getEventManager();

    const workflowExecutionBuilder = await this.getWorkflowExecutionBuilder();

    const buildResult: WorkflowExecutionBuildResult = await workflowExecutionBuilder.build(
      this.params.workflowId,
      this.params.options || {},
    );
    const executionEntity = buildResult.workflowExecutionEntity;
    const executionId = executionEntity.id;

    workflowExecutionRegistry.register(executionEntity);

    const eventQueue: BaseEvent[] = [];
    let resolveEvent: ((value: IteratorResult<BaseEvent>) => void) | null = null;
    let executionComplete = false;

    const eventListener = (event: BaseEvent) => {
      if (event.executionId === executionId) {
        eventQueue.push(event);
        if (resolveEvent) {
          const nextEvent = eventQueue.shift()!;
          resolveEvent({ value: nextEvent, done: false });
          resolveEvent = null;
        }
      }
    };

    const eventTypes: Array<BaseEvent["type"]> = [
      "WORKFLOW_EXECUTION_STARTED",
      "WORKFLOW_EXECUTION_COMPLETED",
      "WORKFLOW_EXECUTION_FAILED",
      "WORKFLOW_EXECUTION_PAUSED",
      "WORKFLOW_EXECUTION_RESUMED",
      "WORKFLOW_EXECUTION_CANCELLED",
      "NODE_STARTED",
      "NODE_COMPLETED",
      "NODE_FAILED",
      "TOOL_CALL_STARTED",
      "TOOL_CALL_COMPLETED",
      "TOOL_CALL_FAILED",
      "ERROR",
    ];

    const unsubscribers: Array<() => void> = [];

    // Use new EventEmitter API
    const emitter = eventManager.getEmitter(executionId);
    for (const eventType of eventTypes) {
      const unsubscribe = emitter.on(eventType, eventListener);
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
            new Promise<IteratorResult<BaseEvent>>(resolve => {
              resolveEvent = resolve;
            }).then(result => {
              if (result.done === false) {
                return result;
              }
              return null;
            }),
          ]).then(result => {
            if (result && typeof result === "object" && "value" in result) {
              return result as IteratorResult<BaseEvent>;
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
    const globalContext = this.dependencies.getGlobalContext();
    return globalContext.container.get(ServiceIdentifiers.WorkflowExecutionBuilder);
  }

  validate(): CommandValidationResult {
    return validateWorkflowExecutionParams(this.params.workflowId);
  }
}
