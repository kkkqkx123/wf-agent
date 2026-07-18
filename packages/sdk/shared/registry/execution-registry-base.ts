/**
 * Base Execution Registry - Shared base class for AgentLoopRegistry and WorkflowExecutionRegistry.
 *
 * Provides common infrastructure:
 * - Entity storage with Map<ID, TEntity>
 * - State coordinator storage with Map<ID, TCoordinator>
 * - Storage adapter support
 * - Basic CRUD operations (register, has, get, getAll, getAllIds, size, clear)
 * - Coordinator lifecycle management
 * - AsyncDispose support
 *
 * Each specific registry extends this base and adds its own typed query methods.
 */

/**
 * Minimal entity interface required by the base registry.
 */
export interface BaseExecutionEntity {
  id: string;
  cleanup(): void;
}

/**
 * Minimal coordinator interface required by the base registry.
 */
export interface BaseCoordinator {
  cleanup(): void;
}

/**
 * Base Execution Registry - Abstract base class.
 *
 * @template TEntity - The entity type (must extend BaseExecutionEntity)
 * @template TCoordinator - The coordinator type (must extend BaseCoordinator)
 * @template TStorageAdapter - The storage adapter type (optional)
 */
export abstract class BaseExecutionRegistry<
  TEntity extends BaseExecutionEntity,
  TCoordinator extends BaseCoordinator,
  TStorageAdapter,
> {
  /** Entity storage */
  protected entities: Map<string, TEntity> = new Map();
  /** State coordinator storage */
  protected coordinators: Map<string, TCoordinator> = new Map();
  /** Storage adapter (optional) */
  protected storageAdapter?: TStorageAdapter;

  constructor(options?: { storageAdapter?: TStorageAdapter }) {
    this.storageAdapter = options?.storageAdapter;
  }

  /**
   * Register an entity and persist to storage (async, non-blocking).
   * Persistence errors are caught and logged internally.
   */
  register(entity: TEntity): void {
    this.entities.set(entity.id, entity);
    this.persistToStorage(entity).catch(() => {
      // Persistence errors are handled by the subclass implementation
    });
  }

  /**
   * Check if an entity exists.
   */
  has(id: string): boolean {
    return this.entities.has(id);
  }

  /**
   * Get all registered entities.
   */
  getAll(): TEntity[] {
    return Array.from(this.entities.values());
  }

  /**
   * Get all registered entity IDs.
   */
  getAllIds(): string[] {
    return Array.from(this.entities.keys());
  }

  /**
   * Get the number of registered entities.
   */
  size(): number {
    return this.entities.size;
  }

  /**
   * Register a state coordinator for an entity.
   */
  registerCoordinator(id: string, coordinator: TCoordinator): void {
    this.coordinators.set(id, coordinator);
  }

  /**
   * Get the state coordinator for an entity.
   */
  getCoordinator(id: string): TCoordinator | null {
    return this.coordinators.get(id) || null;
  }

  /**
   * Clear all entities and coordinators.
   * Calls cleanup on each entity and coordinator before clearing.
   */
  clear(): void {
    for (const entity of this.entities.values()) {
      entity.cleanup();
    }
    for (const coordinator of this.coordinators.values()) {
      coordinator.cleanup();
    }
    this.entities.clear();
    this.coordinators.clear();
  }

  /**
   * Enable await using pattern support.
   * Delegates to clear() for resource release.
   */
  async [Symbol.asyncDispose](): Promise<void> {
    this.clear();
  }

  // ============================================================
  // Abstract Persistence Methods
  // ============================================================

  /**
   * Persist entity to storage (async, fire-and-forget).
   * Subclasses should implement serialization and storage adapter logic.
   * Errors should be caught and logged internally by the implementation.
   */
  protected abstract persistToStorage(entity: TEntity): Promise<void>;

  /**
   * Remove entity from storage (async, fire-and-forget).
   * Subclasses should implement storage adapter deletion logic.
   * Errors should be caught and logged internally by the implementation.
   */
  protected abstract removeFromStorage(id: string): Promise<void>;

  /**
   * Initialize registry from storage.
   * Subclasses should implement the loading logic.
   */
  abstract initializeFromStorage(): Promise<void>;
}