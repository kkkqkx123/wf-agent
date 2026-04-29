/**
 * CreateCheckpointCommand - Create Thread Checkpoint Command
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
  /** Thread ID */
  threadId: string;
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Create Checkpoint Command
 *
 * Workflow:
 * 1. Validate parameters (threadId required)
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
    const threadRegistry = this.dependencies.getThreadRegistry();
    const checkpointStateManager = this.dependencies.getCheckpointStateManager();
    const workflowRegistry = this.dependencies.getWorkflowRegistry();
    const graphRegistry = this.dependencies.getGraphRegistry();

    const threadEntity = threadRegistry.get(this.params.threadId);
    if (!threadEntity) {
      throw new WorkflowExecutionNotFoundError(
        `Thread not found: ${this.params.threadId}`,
        this.params.threadId,
      );
    }

    const dependencies = {
      workflowExecutionRegistry: threadRegistry as unknown as import("../../../../workflow/stores/workflow-execution-registry.js").WorkflowExecutionRegistry,
      checkpointStateManager,
      workflowRegistry,
      workflowGraphRegistry: graphRegistry,
    };

    const checkpointId = await CheckpointCoordinator.createCheckpoint(
      this.params.threadId,
      dependencies,
      this.params.metadata,
    );

    return checkpointId;
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    if (!this.params.threadId || this.params.threadId.trim().length === 0) {
      errors.push("threadId must be provided");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
