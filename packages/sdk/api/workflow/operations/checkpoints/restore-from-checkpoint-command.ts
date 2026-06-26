/**
 * RestoreFromCheckpointCommand - Restore Workflow Execution from Checkpoint Command
 *
 * Category: Management
 * Restores a workflow execution state from a previously created checkpoint
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../../shared/types/command.js";
import { validateCheckpointRestorationParams } from "../../../shared/operations/validators/workflow-validators.js";
import type { CommandValidationResult } from "../../../shared/types/command.js";
import { CheckpointCoordinator } from "../../../../workflow/checkpoint/checkpoint-coordinator.js";
import type { WorkflowExecution } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { WorkflowExecutionRegistry } from "../../../../workflow/stores/workflow-execution-registry.js";

/**
 * Restore from checkpoint command parameters
 */
export interface RestoreFromCheckpointParams {
  /** Checkpoint ID to restore from */
  checkpointId: string;
}

/**
 * RestoreFromCheckpointCommand - Restore workflow execution from a checkpoint
 */
export class RestoreFromCheckpointCommand extends ManagementCommand<WorkflowExecution> {
  constructor(
    private readonly params: RestoreFromCheckpointParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "RestoreFromCheckpointCommand",
      description: "Restore workflow execution state from a checkpoint",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: false,
      idempotent: false,
    };
  }

  /**
   * Verify command parameters using shared validator
   */
  validate(): CommandValidationResult {
    return validateCheckpointRestorationParams(this.params.checkpointId);
  }

  /**
   * Execute the command
   */
  protected async executeInternal(): Promise<WorkflowExecution> {
    const executionRegistry = this.dependencies.getWorkflowExecutionRegistry();
    const dependencies = {
      workflowExecutionRegistry: executionRegistry as unknown as WorkflowExecutionRegistry,
      checkpointStateManager: this.dependencies.getCheckpointStateManager(),
      workflowRegistry: this.dependencies.getWorkflowRegistry(),
      workflowGraphRegistry: this.dependencies.getWorkflowGraphRegistry(),
    };

     const coordinator = new CheckpointCoordinator();
     const { workflowExecutionEntity } = await coordinator.restoreWorkflowFromCheckpoint(
       this.params.checkpointId,
       dependencies,
     );
     return workflowExecutionEntity.getWorkflowExecutionData();
  }
}
