/**
 * RestoreFromCheckpointCommand - Restore a workflow execution from a checkpoint
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import { CheckpointCoordinator } from "../../../../workflow/checkpoint/checkpoint-coordinator.js";
import type { WorkflowExecution } from "@wf-agent/types";

import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Restore parameters from the checkpoint.
 */
export interface RestoreFromCheckpointParams {
  /** Checkpoint ID */
  checkpointId: string;
}

/**
 * RestoreFromCheckpointCommand - Restore a workflow execution from a checkpoint
 */
export class RestoreFromCheckpointCommand extends BaseCommand<WorkflowExecution> {
  constructor(
    private readonly params: RestoreFromCheckpointParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Verify command parameters
   */
  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.checkpointId || this.params.checkpointId.trim() === "") {
      errors.push("Checkpoint ID cannot be null");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }

  /**
   * Execute the command
   */
  protected async executeInternal(): Promise<WorkflowExecution> {
    const executionRegistry = this.dependencies.getWorkflowExecutionRegistry();
    const dependencies = {
      workflowExecutionRegistry: executionRegistry as unknown as import("../../../../workflow/stores/workflow-execution-registry.js").WorkflowExecutionRegistry,
      checkpointStateManager: this.dependencies.getCheckpointStateManager(),
      workflowRegistry: this.dependencies.getWorkflowRegistry(),
      workflowGraphRegistry: this.dependencies.getWorkflowGraphRegistry(),
    };

    const { workflowExecutionEntity } = await CheckpointCoordinator.restoreFromCheckpoint(
      this.params.checkpointId,
      dependencies,
    );
    return workflowExecutionEntity.getExecution();
  }
}
