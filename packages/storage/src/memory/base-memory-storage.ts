/**
 * In-Memory Storage Base Class
 * Fast, isolated storage implementation perfect for testing
 */

import type { BaseStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { StorageError } from "../types/storage-errors.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("memory-storage");

/**
 * Memory Storage Configuration
 */
export interface MemoryStorageConfig {
  /** Simulate I/O latency for realistic testing (default: false) */
  simulateLatency?: boolean;
  /** Latency in milliseconds when simulating (default: 10) */
  latencyMs?: number;
  /** Randomly simulate errors for resilience testing (default: false) */
  simulateErrors?: boolean;
  /** Error probability 0-1 when simulating errors (default: 0.01) */
  errorRate?: number;
}

/**
 * Base in-memory storage implementation
 * Provides fast, isolated storage with optional latency/error simulation
 * @template TMetadata Metadata type
 * @template TListOptions List options type
 */
export abstract class BaseMemoryStorage<TMetadata, TListOptions = any>
  implements BaseStorageAdapter<TMetadata, TListOptions>
{
  protected store: Map<string, { data: Uint8Array; metadata: TMetadata }> = new Map();
  protected initialized: boolean = false;
  protected config: Required<MemoryStorageConfig>;

  constructor(config: MemoryStorageConfig = {}) {
    this.config = {
      simulateLatency: config.simulateLatency ?? false,
      latencyMs: config.latencyMs ?? 10,
      simulateErrors: config.simulateErrors ?? false,
      errorRate: config.errorRate ?? 0.01,
    };
  }

  /**
   * Initialize memory storage
   */
  async initialize(): Promise<void> {
    logger.debug("Initializing memory storage");
    this.initialized = true;
  }

  /**
   * Save data to memory storage
   */
  async save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();
    await this.simulateRandomError("save");

    // Create a copy to prevent external mutation
    this.store.set(id, { data: new Uint8Array(data), metadata });
    logger.debug("Data saved to memory storage", { id, size: data.length });
  }

  /**
   * Load data from memory storage
   */
  async load(id: string): Promise<Uint8Array | null> {
    this.ensureInitialized();
    await this.simulateLatency();
    await this.simulateRandomError("load");

    const entry = this.store.get(id);
    if (!entry) {
      logger.debug("Data not found in memory storage", { id });
      return null;
    }

    // Return a copy to prevent external mutation
    logger.debug("Data loaded from memory storage", { id, size: entry.data.length });
    return new Uint8Array(entry.data);
  }

  /**
   * Delete data from memory storage
   */
  async delete(id: string): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();
    await this.simulateRandomError("delete");

    this.store.delete(id);
    logger.debug("Data deleted from memory storage", { id });
  }

  /**
   * Check if data exists in memory storage
   */
  async exists(id: string): Promise<boolean> {
    this.ensureInitialized();
    await this.simulateLatency();
    return this.store.has(id);
  }

  /**
   * List all IDs in memory storage
   */
  async list(options?: TListOptions): Promise<string[]> {
    this.ensureInitialized();
    await this.simulateLatency();
    return Array.from(this.store.keys());
  }

  /**
   * Clear all data from memory storage
   */
  async clear(): Promise<void> {
    this.ensureInitialized();
    await this.simulateLatency();

    const count = this.store.size;
    this.store.clear();
    logger.info("Memory storage cleared", { removedItems: count });
  }

  /**
   * Close memory storage (clears all data)
   */
  async close(): Promise<void> {
    logger.debug("Closing memory storage");
    this.store.clear();
    this.initialized = false;
  }

  /**
   * Get metadata without loading data
   */
  async getMetadata(id: string): Promise<TMetadata | null> {
    this.ensureInitialized();
    const entry = this.store.get(id);
    return entry?.metadata ?? null;
  }

  /**
   * Get storage statistics
   */
  getStats(): { count: number; totalSize: number } {
    let totalSize = 0;
    for (const entry of this.store.values()) {
      totalSize += entry.data.length;
    }

    return {
      count: this.store.size,
      totalSize,
    };
  }

  /**
   * Ensure storage is initialized
   */
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new StorageError("Memory storage not initialized. Call initialize() first.", "initialize");
    }
  }

  /**
   * Simulate I/O latency if configured
   */
  protected async simulateLatency(): Promise<void> {
    if (this.config.simulateLatency) {
      await new Promise(resolve => setTimeout(resolve, this.config.latencyMs));
    }
  }

  /**
   * Simulate random errors if configured
   */
  private async simulateRandomError(operation: string): Promise<void> {
    if (this.config.simulateErrors && Math.random() < this.config.errorRate) {
      throw new StorageError(
        `Simulated random error during ${operation}`,
        operation,
        { simulated: true, operation },
      );
    }
  }
}
