/**
 * In-Memory Script Storage Adapter
 * Fast, isolated script storage for testing
 */

import type { ScriptStorageMetadata } from "@wf-agent/types";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based script storage implementation
 */
export class MemoryScriptStorage extends BaseMemoryStorage<ScriptStorageMetadata, void> {
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }
}
