/**
 * Thread storage callback interface definition
 * Defines a unified interface for thread persistence operations
 */

import type { ThreadStorageMetadata, ThreadListOptions, ThreadStatus } from "@wf-agent/types";
import type { BaseStorageCallback } from "./base-storage-callback.js";

/**
 * Thread Storage Callback Interface
 *
 * Defines a unified interface for thread persistence operations
 * - Inherits from BaseStorageCallback, providing standard CRUD (Create, Read, Update, Delete) operations
 * - The packages/storage provide implementations of ThreadStorageAdapter based on this interface
 * - The application layer can directly use ThreadStorageAdapter or implement this interface itself
 */
export interface ThreadStorageCallback extends BaseStorageCallback<
  ThreadStorageMetadata,
  ThreadListOptions
> {
  /**
   * Update thread status
   * @param threadId: Unique thread identifier
   * @param status: New status
   */
  updateThreadStatus(threadId: string, status: ThreadStatus): Promise<void>;
}
