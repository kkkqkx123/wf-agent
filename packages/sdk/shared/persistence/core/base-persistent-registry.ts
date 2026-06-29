/**
 * Base Persistent Registry
 *
 * Abstract base class for all registries with persistence support.
 * Provides unified persistence management, event notifications, and consistency verification.
 *
 * Features:
 * - Unified persistence with BLOCKING/ASYNC strategy support
 * - Event notifications for all persistence operations
 * - Data consistency verification
 * - Automatic failure tracking and recovery
 * - Lifecycle hooks
 * - Automatic ID extraction via IdExtractor pattern
 *
 * Usage:
 * ```typescript
 * class MyRegistry extends BasePersistentRegistry<MyEntity> {
 *   protected getIdExtractor(): IdExtractor<MyEntity> {
 *     return { extractId: (entity) => entity.id };
 *   }
 *
 *   protected async serializeEntity(entity: MyEntity): Promise<Uint8Array> {
 *     // Implement serialization
 *   }
 *
 *   protected async buildMetadata(entity: MyEntity): Promise<Record<string, any>> {
 *     // Implement metadata building
 *   }
 *
 *   protected getRegistryName(): string {
 *     return "MyRegistry";
 *   }
 * }
 * ```
 */

import type {
  PersistenceConfig,
  RequiredPersistenceConfig,
  RegistryPersistenceConfig,
  PersistenceHooks,
  IdExtractor,
} from "./types.js";
import { PersistenceStrategy } from "./types.js";
import { PersistenceEventEmitter } from "../../storage/persistence-event-emitter.js";
import { DataConsistencyValidator, type ConsistencyReport } from "../../storage/data-consistency-validator.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

/**
 * Default persistence configuration
 */
const DEFAULT_PERSISTENCE_CONFIG: RequiredPersistenceConfig = {
  strategy: PersistenceStrategy.ASYNC,
  timeoutMs: 30000,
};

/**
 * Merge user config with defaults
 */
function mergePersistenceConfig(userConfig?: PersistenceConfig): RequiredPersistenceConfig {
  return {
    ...DEFAULT_PERSISTENCE_CONFIG,
    ...userConfig,
  };
}

/**
 * Base class for all persistent registries
 */
export abstract class BasePersistentRegistry<T> {
  /** Storage adapter for persistence (optional) */
  protected storageAdapter?: any;

  /** Persistence configuration */
  protected persistenceConfig: RequiredPersistenceConfig;

  /** Event system for persistence notifications */
  protected eventEmitter: PersistenceEventEmitter;

  /** Data consistency validator */
  protected consistencyValidator: DataConsistencyValidator;

  /** In-memory storage */
  protected items: Map<string, T> = new Map();

  /** Lifecycle hooks */
  protected hooks: PersistenceHooks<T> = {};

  /** Logger instance */
  protected logger = createContextualLogger({ component: this.getRegistryName() });

  /**
   * Constructor
   */
  constructor(config?: RegistryPersistenceConfig) {
    this.storageAdapter = config?.storageAdapter;
    this.persistenceConfig = mergePersistenceConfig(config?.persistenceConfig);
    this.eventEmitter = new PersistenceEventEmitter();
    this.consistencyValidator = new DataConsistencyValidator();

    this.logger.info("Registry initialized with persistence", {
      registryName: this.getRegistryName(),
      strategy: this.persistenceConfig.strategy,
      hasStorageAdapter: !!this.storageAdapter,
    });
  }

  /**
   * Register entity (synchronous)
   *
   * Automatically extracts ID from entity using IdExtractor.
   * Registers the entity in memory and initiates asynchronous persistence.
   * In BLOCKING mode, use registerAsync() for guaranteed persistence before returning.
   *
   * @param entity The entity to register
   */
  register(entity: T): void {
    const id = this.getIdExtractor().extractId(entity);
    this.items.set(id, entity);
    // Always persist asynchronously (cannot block in sync method)
    this.persistAsync(entity, id).catch(() => {
      // Error already logged and emitted as event
    });
  }

  /**
   * Register entity (asynchronous)
   *
   * Automatically extracts ID from entity using IdExtractor.
   * Registers the entity in memory and persists based on strategy:
   * - BLOCKING: Waits for persistence to complete
   * - ASYNC: Returns immediately, persistence happens in background
   *
   * @param entity The entity to register
   * @param options Optional configuration for this operation
   * @throws In BLOCKING mode if persistence fails
   */
  async registerAsync(entity: T, options?: { waitForPersistence?: boolean }): Promise<void> {
    const id = this.getIdExtractor().extractId(entity);

    // Call lifecycle hook
    if (this.hooks.onBeforePersist) {
      await this.hooks.onBeforePersist(entity);
    }

    // Register in memory
    this.items.set(id, entity);

    // Determine if we should wait for persistence
    const waitForPersistence =
      options?.waitForPersistence ?? this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING;

    if (waitForPersistence) {
      // BLOCKING mode: Wait for persistence
      await this.persistAsync(entity, id);
    } else {
      // ASYNC mode: Start persistence in background
      this.persistAsync(entity, id).catch(() => {
        // Error already logged and emitted
      });
    }

    // Call lifecycle hook
    if (this.hooks.onAfterPersist) {
      await this.hooks.onAfterPersist(entity);
    }
  }

  /**
   * Unregister entity (synchronous)
   *
   * Removes entity from memory and initiates asynchronous removal from storage.
   *
   * @param id The entity ID
   * @returns Whether the unregister was successful
   */
  unregister(id: string): boolean {
    const result = this.items.delete(id);
    if (result) {
      // Remove from storage asynchronously
      this.removeAsync(id).catch(() => {
        // Error already logged
      });
    }
    return result;
  }

  /**
   * Unregister entity (asynchronous)
   *
   * Removes entity from memory and storage based on strategy:
   * - BLOCKING: Waits for storage removal to complete
   * - ASYNC: Returns immediately, removal happens in background
   *
   * @param id The entity ID
   * @param options Optional configuration for this operation
   * @returns Whether the unregister was successful
   * @throws In BLOCKING mode if removal fails
   */
  async unregisterAsync(id: string, options?: { waitForRemoval?: boolean }): Promise<boolean> {
    // Call lifecycle hook
    if (this.hooks.onBeforeRemove) {
      await this.hooks.onBeforeRemove(id);
    }

    const result = this.items.delete(id);

    if (result) {
      const waitForRemoval =
        options?.waitForRemoval ?? this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING;

      if (waitForRemoval) {
        // BLOCKING mode: Wait for removal
        await this.removeAsync(id);
      } else {
        // ASYNC mode: Start removal in background
        this.removeAsync(id).catch(() => {
          // Error already logged
        });
      }
    }

    // Call lifecycle hook
    if (this.hooks.onAfterRemove) {
      await this.hooks.onAfterRemove(id);
    }

    return result;
  }

  /**
   * Check if entity exists
   */
  has(id: string): boolean {
    return this.items.has(id);
  }

  /**
   * Get entity
   */
  get(id: string): T | null {
    return this.items.get(id) ?? null;
  }

  /**
   * Get all entities
   */
  getAll(): T[] {
    return Array.from(this.items.values());
  }

  /**
   * Get all entity IDs
   */
  getAllIds(): string[] {
    return Array.from(this.items.keys());
  }

  /**
   * Get entity count
   */
  size(): number {
    return this.items.size;
  }

  /**
   * Clear all entities
   */
  clear(): void {
    this.items.clear();
  }

  /**
   * Get the event emitter
   */
  getEventEmitter(): PersistenceEventEmitter {
    return this.eventEmitter;
  }

  /**
   * Set lifecycle hooks
   */
  setHooks(hooks: PersistenceHooks<T>): void {
    this.hooks = { ...this.hooks, ...hooks };
  }

  /**
   * Verify data consistency between memory and storage
   */
  async verifyConsistency(): Promise<ConsistencyReport> {
    const memoryIds = new Set(this.getAllIds());
    const storageIds = new Set<string>();

    if (this.storageAdapter) {
      const ids = await this.storageAdapter.list();
      ids.forEach((id: string) => storageIds.add(id));
    }

    return this.consistencyValidator.verify(this.getRegistryName(), memoryIds, storageIds);
  }

  /**
   * Get consistency validator
   */
  getConsistencyValidator(): DataConsistencyValidator {
    return this.consistencyValidator;
  }

  /**
   * Initialize registry from storage
   *
   * Loads all entities from storage into memory.
   * By default, this is a no-op and should be overridden by subclasses if needed.
   *
   * @returns Summary of initialization
   */
  async initializeFromStorage(): Promise<{
    loadedCount: number;
    failedCount: number;
    totalCount: number;
  }> {
    if (!this.storageAdapter) {
      this.logger.debug("No storage adapter configured, skipping initialization");
      return { loadedCount: 0, failedCount: 0, totalCount: 0 };
    }

    try {
      const ids = await this.storageAdapter.list();
      this.logger.info("Found items in storage for initialization", { count: ids.length });
      return {
        loadedCount: 0,
        failedCount: 0,
        totalCount: ids.length,
      };
    } catch (error) {
      this.logger.error("Failed to initialize registry from storage", {
        error: getErrorMessage(error),
      });
      // Return error status instead of throwing
      // Allows initialization to continue even if storage is unavailable
      return {
        loadedCount: 0,
        failedCount: 0,
        totalCount: 0,
      };
    }
  }

  // ============================================================
  // Protected methods - persistence operations
  // ============================================================

  /**
   * Persist entity to storage
   *
   * @protected Called internally
   */
  protected async persistAsync(entity: T, id: string): Promise<void> {
    if (!this.storageAdapter) {
      this.logger.debug("No storage adapter configured, skipping persistence");
      return;
    }

    try {
      // Notify listeners that persistence is starting
      this.eventEmitter.notifyPersistStarted(id, "save");

      // Serialize and build metadata
      const data = await this.serializeEntity(entity);
      const metadata = await this.buildMetadata(entity);

      // Persist to storage
      await this.storageAdapter.save(id, data, metadata);

      // Notify success
      this.eventEmitter.notifyPersistSuccess(id, "save", data.byteLength);

      this.logger.debug("Entity persisted to storage", { entityId: id });
    } catch (error) {
      const errorObj = error instanceof Error ? error : new Error(String(error));

      // Notify failure (will be tracked for diagnostics)
      this.eventEmitter.notifyPersistFailed(id, "save", errorObj);

      // Error handling depends on persistence strategy
      if (this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING) {
        // In BLOCKING mode, re-throw to make error visible to caller
        this.logger.error("Failed to persist entity to storage (BLOCKING mode)", {
          entityId: id,
          error: getErrorMessage(error),
        });
        throw error;
      } else {
        // In ASYNC mode, just log the error
        // Application must listen to persist_failed events
        this.logger.error("Failed to persist entity to storage (ASYNC mode)", {
          entityId: id,
          error: getErrorMessage(error),
        });
      }
    }
  }

  /**
   * Remove entity from storage
   *
   * @protected Called internally
   */
  protected async removeAsync(id: string): Promise<void> {
    if (!this.storageAdapter) {
      this.logger.debug("No storage adapter configured, skipping removal");
      return;
    }

    try {
      this.eventEmitter.notifyPersistStarted(id, "delete");
      await this.storageAdapter.delete(id);
      this.eventEmitter.notifyPersistSuccess(id, "delete");

      this.logger.debug("Entity removed from storage", { entityId: id });
    } catch (error) {
      const errorObj = error instanceof Error ? error : new Error(String(error));

      this.eventEmitter.notifyPersistFailed(id, "delete", errorObj);

      // Error handling depends on persistence strategy
      if (this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING) {
        this.logger.error("Failed to remove entity from storage (BLOCKING mode)", {
          entityId: id,
          error: getErrorMessage(error),
        });
        throw error;
      } else {
        this.logger.error("Failed to remove entity from storage (ASYNC mode)", {
          entityId: id,
          error: getErrorMessage(error),
        });
      }
    }
  }

  // ============================================================
  // Abstract methods - must be implemented by subclasses
  // ============================================================

  /**
   * Get ID extractor for this entity type
   *
   * Defines how to extract the ID from an entity.
   * This allows the base class to automatically handle ID extraction.
   *
   * @abstract
   * @returns IdExtractor instance
   */
  protected abstract getIdExtractor(): IdExtractor<T>;

  /**
   * Serialize entity to bytes
   *
   * @abstract
   */
  protected abstract serializeEntity(entity: T): Promise<Uint8Array>;

  /**
   * Build storage metadata for entity
   *
   * @abstract
   */
  protected abstract buildMetadata(entity: T): Promise<Record<string, any>>;

  /**
   * Get registry name for logging and diagnostics
   *
   * @abstract
   */
  protected abstract getRegistryName(): string;
}

export { DEFAULT_PERSISTENCE_CONFIG, mergePersistenceConfig };
export type { IdExtractor };
