/**
 * Workflow Execution storage adapter interface definition
 * Defines a unified interface for workflow execution persistence operations
 */

import type { WorkflowExecutionStorageMetadata, WorkflowExecutionListOptions, WorkflowExecutionStatus } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Workflow Execution Storage Adapter Interface
 *
 * Defines a unified interface for workflow execution persistence operations
 * - Inherits from BaseStorageAdapter, providing standard CRUD (Create, Read, Update, Delete) operations
 * - The packages/storage provide implementations of WorkflowExecutionStorageAdapter based on this interface
 * - The application layer can directly use WorkflowExecutionStorageAdapter or implement this interface itself
 */
export interface WorkflowExecutionStorageAdapter extends BaseStorageAdapter<
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
