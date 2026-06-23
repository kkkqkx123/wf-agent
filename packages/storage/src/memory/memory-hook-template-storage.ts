/**
 * In-Memory Hook Template Storage Adapter
 * Fast, isolated hook template storage for testing
 */

import type { HookTemplateStorageMetadata } from "@wf-agent/types";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based hook template storage implementation
 */
export class MemoryHookTemplateStorage extends BaseMemoryStorage<HookTemplateStorageMetadata, void> {
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }
}
