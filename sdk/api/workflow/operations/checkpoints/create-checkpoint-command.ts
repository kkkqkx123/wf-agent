/**
 * CreateCheckpointCommand - Create Workflow Execution Checkpoint Command
 *
 * Responsibilities:
 * - Encapsulates checkpoint creation operation as Command pattern
 * - Provides unified API layer interface
 * - Supports parameter validation
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for APIDependencyManager
 * - Parameter validation is completed in validate() method
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../../shared/types/command.js";
import type { CheckpointMetadata } from "@wf-agent/types";
import { CheckpointCoordinator } from "../../../../workflow/checkpoint/checkpoint-coordinator.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import { WorkflowExecutionNotFoundError } from "@wf-agent/types";

/**
 * Create Checkpoint Command Parameters
 */
export interface CreateCheckpointParams {
  /** Execution ID */
  executionId: string;
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Create Checkpoint Command
 *
 * Workflow:
 * 1. Validate parameters (executionId required)
 * 2. Get WorkflowExecutionEntity from WorkflowExecutionRegistry
 * 3. Call CheckpointCoordinator to create checkpoint
 * 4. Return the checkpoint ID
 */
export class CreateCheckpointCommand extends BaseCommand<string> {
  constructor(
    private readonly params: CreateCheckpointParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<string> {
    const executionRegistry = this.dependencies.getWorkflowExecutionRegistry();
    const checkpointStateManager = this.dependencies.getCheckpointStateManager();
    const workflowRegistry = this.dependencies.getWorkflowRegistry();
    const graphRegistry = this.dependencies.getWorkflowGraphRegistry();

    const executionEntity = executionRegistry.get(this.params.executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(
        `Workflow execution not found: ${this.params.executionId}`,
        this.params.executionId,
      );
    }

    const dependencies = {
      workflowExecutionRegistry: executionRegistry as unknown as import("../../../../workflow/stores/workflow-execution-registry.js").WorkflowExecutionRegistry,
      checkpointStateManager,
      workflowRegistry,
      workflowGraphRegistry: graphRegistry,
    };

    const checkpointId = await CheckpointCoordinator.createCheckpoint(
      this.params.executionId,
      dependencies,
      this.params.metadata,
    );

    return checkpointId;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.executionId || this.params.executionId.trim().length === 0) {
      errors.push("executionId must be provided");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
