/**
 * ExecutionStore & CoordinatorStore
 *
 * Lightweight, focused replacements for the BaseExecutionRegistry abstract class.
 *
 * ExecutionStore - Pure entity storage with cleanup lifecycle.
 * CoordinatorStore - Pure coordinator storage with cleanup lifecycle.
 *
 * Each specific execution registry (AgentLoopRegistry, WorkflowExecutionRegistry)
 * composes these stores instead of extending a monolithic base class.
 */

/**
 * Minimal entity interface required by ExecutionStore.
 */
export interface BaseExecutionEntity {
  id: string;
  cleanup(): void;
}

/**
 * Minimal coordinator interface required by CoordinatorStore.
 */
export interface BaseCoordinator {
  cleanup(): void;
}

/**
 * ExecutionStore - Pure entity storage with cleanup lifecycle.
 *
 * Responsibilities:
 * - Store/fetch/delete entities by ID
 * - Call cleanup() on each entity on clear()
 * - No persistence logic, no storage adapter
 *
 * @template TEntity - The entity type (must extend BaseExecutionEntity)
 */
export class ExecutionStore<TEntity extends BaseExecutionEntity> {
  protected entities = new Map<string, TEntity>();

  /**
   * Register an entity.
   */
  register(entity: TEntity): void {
    this.entities.set(entity.id, entity);
  }

  /**
   * Get an entity by ID.
   */
  get(id: string): TEntity | undefined {
    return this.entities.get(id);
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
   * Delete an entity by ID.
   */
  delete(id: string): void {
    this.entities.delete(id);
  }

  /**
   * Clear all entities, calling cleanup() on each.
   */
  clear(): void {
    for (const entity of this.entities.values()) {
      entity.cleanup();
    }
    this.entities.clear();
  }
}

/**
 * CoordinatorStore - Pure coordinator storage with cleanup lifecycle.
 *
 * Responsibilities:
 * - Store/fetch/delete coordinators by ID
 * - Call cleanup() on each coordinator on clear()
 * - No entity management, no persistence logic
 *
 * @template TCoordinator - The coordinator type (must extend BaseCoordinator)
 */
export class CoordinatorStore<TCoordinator extends BaseCoordinator> {
  protected coordinators = new Map<string, TCoordinator>();

  /**
   * Register a coordinator for an entity.
   */
  register(id: string, coordinator: TCoordinator): void {
    this.coordinators.set(id, coordinator);
  }

  /**
   * Get the coordinator for an entity.
   */
  get(id: string): TCoordinator | null {
    return this.coordinators.get(id) || null;
  }

  /**
   * Delete a coordinator by ID.
   */
  delete(id: string): void {
    this.coordinators.delete(id);
  }

  /**
   * Clear all coordinators, calling cleanup() on each.
   */
  clear(): void {
    for (const coordinator of this.coordinators.values()) {
      coordinator.cleanup();
    }
    this.coordinators.clear();
  }
}