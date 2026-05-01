/**
 * JSON File Checkpoint Storage Implementation
 * Checkpoint persistent storage based on JSON file system with metadata-data separation
 */

import * as path from "path";
import type { CheckpointStorageMetadata, CheckpointStorageListOptions } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "../types/adapter/checkpoint-adapter.js";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";

/**
 * JSON File Checkpoint Storage
 * Implements the CheckpointStorageAdapter interface
 */
export class JsonCheckpointStorage
  extends BaseJsonStorage<CheckpointStorageMetadata>
  implements CheckpointStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for checkpoints
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "checkpoint");
  }

  /**
   * Get data directory path for checkpoints
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "checkpoint");
  }

  /**
   * List checkpoint IDs
   */
  async list(options?: CheckpointStorageListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    // Apply filtering
    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.executionId && metadata.executionId !== options.executionId) {
          return false;
        }

        if (options.workflowId && metadata.workflowId !== options.workflowId) {
          return false;
        }

        if (options.tags && options.tags.length > 0) {
          if (!metadata.tags || !options.tags.some(tag => metadata.tags!.includes(tag))) {
            return false;
          }
        }

        return true;
      });
    }

    // Sort by timestamp descending
    ids.sort((a, b) => {
      const metaA = this["metadataIndex"].get(a)?.metadata;
      const metaB = this["metadataIndex"].get(b)?.metadata;
      return (metaB?.timestamp ?? 0) - (metaA?.timestamp ?? 0);
    });

    // Pagination
    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }
}
