/**
 * Workflow Storage Callback Interface Definition
 * Define a uniform interface for workflow persistence operations
 */

import type {
  WorkflowStorageMetadata,
  WorkflowListOptions,
  WorkflowVersionInfo,
  WorkflowVersionListOptions,
} from "@wf-agent/types";
import type { BaseStorageCallback } from "./base-storage-callback.js";

/**
 * Workflow Storage Callback Interface
 *
 * Defines a unified interface for workflow persistence operations
 * - Inherits from BaseStorageCallback and provides standard CRUD operations.
 * - packages/storage provides a WorkflowStorageAdapter implementation based on this interface.
 * - The application layer can use WorkflowStorageAdapter directly or implement it by itself.
 */
export interface WorkflowStorageCallback extends BaseStorageCallback<
  WorkflowStorageMetadata,
  WorkflowListOptions
> {
  /**
   * Update workflow metadata
   * @param workflowId workflow unique identifier
   * @param metadata Partial metadata update
   */
  updateWorkflowMetadata(
    workflowId: string,
    metadata: Partial<WorkflowStorageMetadata>,
  ): Promise<void>;

  /**
   * Save workflow version
   * @param workflowId workflow unique identifier
   * @param version Version number
   * @param data Serialized workflow data
   * @param changeNote changeNote
   */
  saveWorkflowVersion(
    workflowId: string,
    version: string,
    data: Uint8Array,
    changeNote?: string,
  ): Promise<void>;

  /**
   * List workflow versions
   * @param workflowId workflow unique identifier
   * @param options query options
   * @returns List of version information
   */
  listWorkflowVersions(
    workflowId: string,
    options?: WorkflowVersionListOptions,
  ): Promise<WorkflowVersionInfo[]>;

  /**
   * Load the specified version of the workflow
   * @param workflowId workflow unique identifier
   * @param version version number
   * @returns the workflow data, if it does not exist return null
   */
  loadWorkflowVersion(workflowId: string, version: string): Promise<Uint8Array | null>;

  /**
   * Deleting a workflow version
   * @param workflowId workflow unique identifier
   * @param version Version number
   */
  deleteWorkflowVersion(workflowId: string, version: string): Promise<void>;
}
