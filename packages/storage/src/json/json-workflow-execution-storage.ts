/**
 * JSON File Workflow Execution Storage Implementation
 * Workflow execution persistence storage based on JSON file system with metadata-data separation
 */

import * as path from "path";
import type { WorkflowExecutionStorageMetadata, WorkflowExecutionListOptions, WorkflowExecutionStatus } from "@wf-agent/types";
import type { WorkflowExecutionStorageCallback } from "../types/callback/workflow-execution-callback.js";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";
import { StorageError } from "../types/storage-errors.js";

/**
 * JSON File Workflow Execution Storage
 * Implements the WorkflowExecutionStorageCallback interface
 */
export class JsonWorkflowExecutionStorage
  extends BaseJsonStorage<WorkflowExecutionStorageMetadata>
  implements WorkflowExecutionStorageCallback
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for workflow executions
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "workflow-execution");
  }

  /**
   * Get data directory path for workflow executions
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "workflow-execution");
  }

  /**
   * List workflow execution IDs
   */
  async list(options?: WorkflowExecutionListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    // Apply filtering
    if (options) {
      ids = ids.filter(id => {
        const entry = this["metadataIndex"].get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.workflowId && metadata.workflowId !== options.workflowId) {
          return false;
        }

        if (options.status) {
          if (Array.isArray(options.status)) {
            if (!options.status.includes(metadata.status)) {
              return false;
            }
          } else if (metadata.status !== options.status) {
            return false;
          }
        }

        if (options.executionType && metadata.executionType !== options.executionType) {
          return false;
        }

        if (options.parentExecutionId && metadata.parentExecutionId !== options.parentExecutionId) {
          return false;
        }

        if (options.startTimeFrom && metadata.startTime < options.startTimeFrom) {
          return false;
        }

        if (options.startTimeTo && metadata.startTime > options.startTimeTo) {
          return false;
        }

        if (
          options.endTimeFrom &&
          (metadata.endTime === undefined || metadata.endTime < options.endTimeFrom)
        ) {
          return false;
        }

        if (
          options.endTimeTo &&
          (metadata.endTime === undefined || metadata.endTime > options.endTimeTo)
        ) {
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

    // Sort
    const sortBy = options?.sortBy ?? "startTime";
    const sortOrder = options?.sortOrder ?? "desc";

    ids.sort((a, b) => {
      const metaA = this["metadataIndex"].get(a)?.metadata;
      const metaB = this["metadataIndex"].get(b)?.metadata;

      let valueA: number;
      let valueB: number;

      switch (sortBy) {
        case "startTime":
          valueA = metaA?.startTime ?? 0;
          valueB = metaB?.startTime ?? 0;
          break;
        case "endTime":
          valueA = metaA?.endTime ?? 0;
          valueB = metaB?.endTime ?? 0;
          break;
        case "updatedAt":
        default:
          valueA = metaA?.startTime ?? 0;
          valueB = metaB?.startTime ?? 0;
          break;
      }

      return sortOrder === "asc" ? valueA - valueB : valueB - valueA;
    });

    // Pagination
    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }

  /**
   * Update workflow execution status
   * Only updates metadata file, no need to touch data file
   */
  async updateExecutionStatus(executionId: string, status: WorkflowExecutionStatus): Promise<void> {
    this.ensureInitialized();

    const indexEntry = this["metadataIndex"].get(executionId);
    if (!indexEntry) {
      throw new StorageError(`Workflow execution not found: ${executionId}`, "updateStatus", { executionId });
    }

    // Update metadata only
    const updatedMetadata: WorkflowExecutionStorageMetadata = {
      ...indexEntry.metadata,
      status,
    };

    // Save with existing data
    const data = await this.load(executionId);
    if (data) {
      await this.save(executionId, data, updatedMetadata);
    }
  }
}
