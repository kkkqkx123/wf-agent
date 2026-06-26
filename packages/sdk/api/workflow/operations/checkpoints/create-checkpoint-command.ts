/**
 * CreateCheckpointCommand - Create Workflow Execution Checkpoint Command
 *
 * Category: Management
 * Creates a checkpoint of the current workflow execution state
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateCheckpointCreationParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import type { CheckpointMetadata } from "@wf-agent/types";
import { WorkflowExecutionNotFoundError } from "@wf-agent/types";
import { CheckpointCoordinator } from "../../../../workflow/checkpoint/checkpoint-coordinator.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { WorkflowExecutionRegistry } from "../../../../workflow/stores/workflow-execution-registry.js";

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
 */
export class CreateCheckpointCommand extends ManagementCommand<string> {
  constructor(
    private readonly params: CreateCheckpointParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "CreateCheckpointCommand",
      description: "Create a checkpoint of workflow execution state",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: false,
      idempotent: false,
    };
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
      workflowExecutionRegistry: executionRegistry as unknown as WorkflowExecutionRegistry,
      checkpointStateManager,
      workflowRegistry,
      workflowGraphRegistry: graphRegistry,
    };

     const coordinator = new CheckpointCoordinator();
     const checkpointId = await coordinator.createWorkflowCheckpoint(
       executionEntity,
       dependencies,
       { metadata: this.params.metadata },
     );

    return checkpointId;
  }

  validate(): CommandValidationResult {
    return validateCheckpointCreationParams(this.params.executionId);
  }
}
