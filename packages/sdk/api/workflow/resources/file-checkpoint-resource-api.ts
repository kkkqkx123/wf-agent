/**
 * FileCheckpointResourceAPI - File Checkpoint Resource Management API
 *
 * Provides APIs for managing workspace file state checkpoints.
 * Extends the shared BaseFileCheckpointResourceAPI with workflow-specific type annotations.
 * This API is only available when file checkpointing is enabled in SDKOptions.
 */

import { BaseFileCheckpointResourceAPI } from "../../shared/resources/file-checkpoint-base.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { FileCheckpointMetadata } from "@wf-agent/types";

/**
 * File checkpoint filter options
 */
export interface FileCheckpointFilter {
  /** Filter by entity ID */
  entityId?: string;
  /** Filter by checkpoint type */
  type?: "full" | "incremental";
  /** Maximum number of results */
  limit?: number;
}

/**
 * FileCheckpointResourceAPI - File checkpoint resource management API
 *
 * Handles all CRUD operations, createFileCheckpoint, restoreFileCheckpoint,
 * listByEntity, deleteByEntity, initialize, and close.
 */
export class FileCheckpointResourceAPI extends BaseFileCheckpointResourceAPI<FileCheckpointFilter> {
  constructor(deps: APIDependencyManager) {
    super(deps);
  }

  protected override applyFilter(
    checkpoints: FileCheckpointMetadata[],
    filter: FileCheckpointFilter,
  ): FileCheckpointMetadata[] {
    return checkpoints.filter(cp => {
      if (filter.entityId && cp.entityId !== filter.entityId) {
        return false;
      }
      if (filter.type && cp.type !== filter.type) {
        return false;
      }
      return true;
    });
  }
}