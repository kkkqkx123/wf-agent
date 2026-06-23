/**
 * In-Memory Trigger Storage Adapter
 * Fast, isolated trigger template storage for testing
 */

import type { TriggerStorageMetadata } from "@wf-agent/types";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based trigger storage implementation
 */
export class MemoryTriggerStorage extends BaseMemoryStorage<TriggerStorageMetadata, void> {
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }
}
