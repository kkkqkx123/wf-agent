/**
 * In-Memory Agent Profile Storage Adapter
 * Fast, isolated agent profile storage for testing
 */

import type { AgentProfileStorageMetadata } from "@wf-agent/types";
import { BaseMemoryStorage, type MemoryStorageConfig } from "./base-memory-storage.js";

/**
 * Memory-based agent profile storage implementation
 */
export class MemoryAgentProfileStorage extends BaseMemoryStorage<AgentProfileStorageMetadata, void> {
  constructor(config: MemoryStorageConfig = {}) {
    super(config);
  }
}
