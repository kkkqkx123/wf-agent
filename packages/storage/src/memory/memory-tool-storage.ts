/**
 * In-Memory Tool Storage Adapter
 * Fast, isolated tool storage for testing
 */

import type { ToolStorageMetadata } from "@wf-agent/types";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based tool storage implementation
 */
export class MemoryToolStorage extends BaseMemoryStorage<ToolStorageMetadata, void> {
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }
}
