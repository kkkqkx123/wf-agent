/**
 * General Storage Callback Interface Definition
 * Provides an abstract base class for storage operations to reduce duplicate code.
 */

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
 * General Storage Callback Interface
 *
 * Provides standardized CRUD operation interfaces
 * @template TMetadata - Metadata type
 * @template TListOptions - List query option type
 */
export interface BaseStorageCallback<TMetadata, TListOptions> extends StorageLifecycle {
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
}
