/**
 * General Storage Adapter Interface Definition
 * Provides an abstract base class for storage operations to reduce duplicate code.
 * 
 * @deprecated Use BaseStorageAdapter instead. This alias will be removed in a future version.
 */

import type { StorageMetrics } from "../metrics.js";

/**
 * Storage Lifecycle Interface
 *
 * Defines the basic lifecycle management methods for storage adapters
 * All storage adapters should implement this interface
 */
export interface StorageLifecycle {
  /**
   * Initialize storage
   * Create the necessary resources (directories, database connections, etc.)
   */
  initialize(): Promise<void>;

  /**
   * Close the storage connection
   * Release resources and clean up the state
   */
  close(): Promise<void>;

  /**
   * Clear all data.
   */
  clear(): Promise<void>;
}

/**
 * General Storage Adapter Interface
 *
 * Provides standardized CRUD operation interfaces
 * @template TMetadata - Metadata type
 * @template TListOptions - List query option type
 */
export interface BaseStorageAdapter<TMetadata, TListOptions> extends StorageLifecycle {
  /**
   * Save data
   * @param id: Unique identifier
   * @param data: Serialized data (byte array)
   * @param metadata: Metadata (used for indexing and querying)
   */
  save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void>;

  /**
   * Load data
   * @param id: Unique identifier
   * @returns: Data (array of bytes); returns null if not found
   */
  load(id: string): Promise<Uint8Array | null>;

  /**
   * Delete data
   * @param id Unique identifier
   */
  delete(id: string): Promise<void>;

  /**
   * List all IDs
   * @param options Query options (support multi-dimensional filtering and pagination)
   * @returns Array of IDs
   */
  list(options?: TListOptions): Promise<string[]>;

  /**
   * Check if it exists
   * @param id: Unique identifier
   * @returns: Whether it exists or not
   */
  exists(id: string): Promise<boolean>;

  /**
   * Get metadata
   * @param id: Unique identifier
   * @returns: Metadata; returns null if not found
   */
  getMetadata(id: string): Promise<TMetadata | null>;

  /**
   * Get storage metrics
   * @returns Storage metrics including operation counts, timings, and sizes
   */
  getMetrics(): Promise<StorageMetrics>;

  /**
   * Reset metrics counters
   * Preserves size information but resets operation counts and timings
   */
  resetMetrics(): void;

  /**
   * Save multiple items in a single transaction
   * More efficient than individual saves for bulk operations
   * @param items Array of items to save with id, data, and metadata
   */
  saveBatch(items: Array<{
    id: string;
    data: Uint8Array;
    metadata: TMetadata;
  }>): Promise<void>;

  /**
   * Load multiple items efficiently
   * @param ids Array of IDs to load
   * @returns Array of loaded data (null if not found), maintaining order
   */
  loadBatch(ids: string[]): Promise<Array<{
    id: string;
    data: Uint8Array | null;
  }>>;

  /**
   * Delete multiple items in a single transaction
   * More efficient than individual deletes for bulk operations
   * @param ids Array of IDs to delete
   */
  deleteBatch(ids: string[]): Promise<void>;
}
