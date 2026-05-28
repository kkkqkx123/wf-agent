/**
 * File Checkpoint Storage Adapter Interface
 *
 * Re-exports the canonical interface from @wf-agent/common-utils,
 * extended with StorageLifecycle for storage layer compatibility.
 */

import type { FileCheckpointStorageAdapter as FCStorageAdapter } from "@wf-agent/common-utils";
import type { StorageLifecycle } from "./base-storage-adapter.js";

/**
 * File checkpoint storage adapter interface (storage layer)
 * Extends the canonical interface from common-utils with StorageLifecycle
 */
export interface FileCheckpointStorageAdapter extends FCStorageAdapter, StorageLifecycle {}