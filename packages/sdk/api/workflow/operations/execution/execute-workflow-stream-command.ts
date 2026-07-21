/**
 * ExecuteWorkflowStreamCommand - Execute Workflow Stream Command
 *
 * Category: Execution (Streaming)
 * Executes workflow while streaming events in real-time.
 * Extends StreamingExecutionBase for shared event streaming infrastructure.
 */

import {
  StreamingCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateWorkflowExecutionParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { WorkflowExecutionOptions, BaseEvent, EventType } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { WorkflowExecutionBuildResult } from "../../../../workflow/execution/factories/workflow-execution-builder.js";
import * as ServiceIdentifiers from "../../../../di/service-identifiers.js";
import {
  StreamingExecutionBase,
  type ExecutionContext,
  type EventEmitterLike,
} from "../../../shared/operations/streaming-execution-base.js";

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
 * Executes workflow and yields events as they occur.
 * Uses the shared StreamingExecutionBase for event management.
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
    const streamExecutor = new WorkflowStreamExecutor(
      this.params,
      this.dependencies,
    );
    return streamExecutor.executeStream();
  }

  validate(): CommandValidationResult {
    return validateWorkflowExecutionParams(this.params.workflowId);
  }
}

/**
 * WorkflowStreamExecutor - Internal streaming executor for workflow
 * Extends StreamingExecutionBase to leverage shared event streaming infrastructure.
 */
class WorkflowStreamExecutor extends StreamingExecutionBase {
  constructor(
    private readonly params: ExecuteWorkflowStreamParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async startExecution(): Promise<ExecutionContext> {
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

    const emitter: EventEmitterLike = {
      on: (eventType: EventType, listener: (event: BaseEvent) => void) => {
        const actualEmitter = eventManager.getEmitter(executionId);
        return actualEmitter.on(eventType, listener);
      },
    };

    const executionPromise: Promise<void> = lifecycleCoordinator
      .execute(this.params.workflowId, this.params.options || {})
      .then(() => {});

    return { executionId, emitter, executionPromise };
  }

  protected getEventTypes(): EventType[] {
    return [
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
  }

  private async getWorkflowExecutionBuilder() {
    const globalContext = this.dependencies.getGlobalContext();
    return globalContext.container.get(ServiceIdentifiers.WorkflowExecutionBuilder);
  }
}