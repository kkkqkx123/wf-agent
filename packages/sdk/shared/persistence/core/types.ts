/**
 * Unified Persistence Framework - Type Definitions
 *
 * Defines common types used across the persistence framework
 */

/**
 * Persistence configuration
 */
export enum PersistenceStrategy {
  /** Asynchronous, non-blocking persistence */
  ASYNC = "async",
  /** Synchronous, blocking persistence */
  BLOCKING = "blocking",
}

export interface PersistenceConfig {
  strategy?: PersistenceStrategy;
  timeoutMs?: number;
}

export interface RequiredPersistenceConfig {
  strategy: PersistenceStrategy;
  timeoutMs: number;
}

/**
 * Registry configuration for persistence
 */
export interface RegistryPersistenceConfig {
  storageAdapter?: any; // StorageAdapter
  persistenceConfig?: PersistenceConfig;
  registryName: string;
}

/**
 * Metadata builder interface
 */
export interface MetadataBuilder<T, M extends Record<string, any> = Record<string, any>> {
  build(entity: T): Promise<M> | M;
}

/**
 * Entity serializer interface
 */
export interface EntitySerializer<T> {
  serialize(entity: T): Promise<Uint8Array> | Uint8Array;
  deserialize(data: Uint8Array): Promise<T> | T;
}

/**
 * ID extractor interface
 *
 * Defines how to extract the ID from an entity.
 * This allows the base class to automatically handle ID extraction
 * without requiring subclasses to manually pass IDs.
 */
export interface IdExtractor<T> {
  extractId(entity: T): string;
}

/**
 * Persistence lifecycle hooks
 */
export interface PersistenceHooks<T> {
  onBeforePersist?: (entity: T) => void | Promise<void>;
  onAfterPersist?: (entity: T) => void | Promise<void>;
  onBeforeRemove?: (id: string) => void | Promise<void>;
  onAfterRemove?: (id: string) => void | Promise<void>;
}
