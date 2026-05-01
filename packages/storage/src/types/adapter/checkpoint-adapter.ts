/**
 * Checkpoint Storage Adapter Interface Definition
 * Defines a uniform interface for checkpoint persistence operations
 */

import type { CheckpointStorageMetadata, CheckpointStorageListOptions } from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Checkpoint Storage Adapter Interface
 *
 * Unified interface for defining checkpoint persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - packages/storage provides an implementation of CheckpointStorageAdapter based on this interface.
 * - Applications can use CheckpointStorageAdapter directly or implement it themselves.
 */
export type CheckpointStorageAdapter = BaseStorageAdapter<
  CheckpointStorageMetadata,
  CheckpointStorageListOptions
>;
