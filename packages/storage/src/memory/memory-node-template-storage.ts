/**
 * In-Memory Node Template Storage Adapter
 * Fast, isolated node template storage for testing
 */

import type { NodeTemplateStorageMetadata } from "@wf-agent/types";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based node template storage implementation
 */
export class MemoryNodeTemplateStorage extends BaseMemoryStorage<NodeTemplateStorageMetadata, void> {
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }
}
