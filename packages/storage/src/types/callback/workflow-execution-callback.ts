/**
 * Workflow Execution storage callback interface definition
 * Defines a unified interface for workflow execution persistence operations
 */

import type { WorkflowExecutionStorageMetadata, WorkflowExecutionListOptions, WorkflowExecutionStatus } from "@wf-agent/types";
import type { BaseStorageCallback } from "./base-storage-callback.js";

/**
 * Workflow Execution Storage Callback Interface
 *
 * Defines a unified interface for workflow execution persistence operations
 * - Inherits from BaseStorageCallback, providing standard CRUD (Create, Read, Update, Delete) operations
 * - The packages/storage provide implementations of WorkflowExecutionStorageAdapter based on this interface
 * - The application layer can directly use WorkflowExecutionStorageAdapter or implement this interface itself
 */
export interface WorkflowExecutionStorageCallback extends BaseStorageCallback<
  WorkflowExecutionStorageMetadata,
  WorkflowExecutionListOptions
> {
  /**
   * Update workflow execution status
   * @param executionId: Unique execution identifier
   * @param status: New status
   */
  updateExecutionStatus(executionId: string, status: WorkflowExecutionStatus): Promise<void>;
}
