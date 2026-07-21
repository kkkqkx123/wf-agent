/**
 * FileCheckpointResourceAPI - File Checkpoint Resource Management API
 *
 * Provides APIs for managing workspace file state checkpoints.
 * Extends the shared BaseFileCheckpointResourceAPI with workflow-specific type annotations.
 * This API is only available when file checkpointing is enabled in SDKOptions.
 */

import { BaseFileCheckpointResourceAPI, BaseFileCheckpointFilter } from "../../shared/resources/file-checkpoint-base.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { FileCheckpointInfo } from "@wf-agent/types";

/**
 * File checkpoint filter options
 */
export interface FileCheckpointFilter extends BaseFileCheckpointFilter {}

/**
 * FileCheckpointResourceAPI - File checkpoint resource management API
 *
 * Provides APIs for managing workspace file state checkpoints.
 * Extends the shared BaseFileCheckpointResourceAPI with workflow-specific type annotations.
 * This API is only available when file checkpointing is enabled in SDKOptions.
 */
export class FileCheckpointResourceAPI extends BaseFileCheckpointResourceAPI<FileCheckpointFilter> {
  constructor(deps: APIDependencyManager) {
    super(deps);
  }

  /**
   * List file checkpoints for a specific workflow execution
   *
   * @param executionId - Workflow execution ID to list checkpoints for
   * @param options - Additional options (limit)
   * @returns Array of checkpoint info
   */
  async listByWorkflowExecution(
    executionId: string,
    options?: { limit?: number },
  ): Promise<FileCheckpointInfo[]> {
    return this.listByEntity(executionId, options);
  }

  /**
   * Delete all file checkpoints for a specific workflow execution
   *
   * @param executionId - Workflow execution ID
   * @param keepLatest - Number of latest checkpoints to keep
   * @returns Number of deleted checkpoints
   */
  async deleteByWorkflowExecution(executionId: string, keepLatest?: number): Promise<number> {
    return this.deleteByEntity(executionId, keepLatest);
  }
}