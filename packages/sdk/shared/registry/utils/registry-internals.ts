/**
 * Registry Internals
 *
 * Provides factory functions and base classes for creating type-safe in-memory registries.
 * Uses composition over inheritance for better flexibility and simplicity.
 * RegistryImpl and PersistentRegistryImpl provide class-based implementations for registries
 * that need to extend base functionality.
 */

import type { MutableRegistry, Registry } from "../types.js";

export type { Registry, MutableRegistry };

/**
 * Creates a new mutable registry with the specified initial items.
 */
export function createRegistry<T>(initialItems?: Iterable<[string, T]>): MutableRegistry<T> {
  const items = new Map<string, T>(initialItems);

  return {
    get: (key: string) => items.get(key),
    has: (key: string) => items.has(key),
    list: () => Array.from(items.values()),
    keys: () => Array.from(items.keys()),
    get size() {
      return items.size;
    },
    clear: () => items.clear(),
    set: (key: string, value: T) => items.set(key, value),
    delete: (key: string) => items.delete(key),
  };
}

/**
 * RegistryImpl - Base class implementing MutableRegistry<T>.
 *
 * Provides a class-based alternative to createRegistry() for registries that need
 * to extend base functionality (e.g., add dependency tracking, validation, or custom methods).
 *
 * Usage:
 * ```typescript
 * class MyRegistry extends RegistryImpl<MyType> {
 *   // Inherits get/has/list/keys/size/set/delete/clear
 *   // Add custom methods here
 * }
 * ```
 */
export class RegistryImpl<T> implements MutableRegistry<T> {
  protected items = new Map<string, T>();

  get(key: string): T | undefined {
    return this.items.get(key);
  }

  has(key: string): boolean {
    return this.items.has(key);
  }

  list(): T[] {
    return Array.from(this.items.values());
  }

  keys(): string[] {
    return Array.from(this.items.keys());
  }

  get size(): number {
    return this.items.size;
  }

  set(key: string, value: T): void {
    this.items.set(key, value);
  }

  delete(key: string): boolean {
    return this.items.delete(key);
  }

  clear(): void {
    this.items.clear();
  }
}

/**
 * PersistentRegistryImpl - Extends RegistryImpl with storage persistence support.
 *
 * Provides a base class for registries that need to persist items to a storage adapter.
 * Subclasses can override save/load/delete methods for custom serialization logic.
 *
 * Usage:
 * ```typescript
 * class MyRegistry extends PersistentRegistryImpl<MyType> {
 *   constructor(storageAdapter: MyStorageAdapter | null) {
 *     super(storageAdapter);
 *   }
 *   // Override save/load/delete for custom serialization
 * }
 * ```
 */
export class PersistentRegistryImpl<T> extends RegistryImpl<T> {
  constructor(protected storageAdapter: { save?(key: string, data: Uint8Array, metadata?: unknown): Promise<void>; load?(key: string): Promise<Uint8Array | null>; delete?(key: string): Promise<void>; list?(): Promise<string[]>; clear?(): Promise<void> } | null = null) {
    super();
  }

  /**
   * Persist an item to storage (async, fire-and-forget by default).
   * Subclasses should override for custom serialization.
   */
  async save(key: string, value: T): Promise<void> {
    if (!this.storageAdapter?.save) return;
    const encoder = new TextEncoder();
    await this.storageAdapter.save(key, encoder.encode(JSON.stringify(value)));
  }

  /**
   * Load an item from storage.
   * Subclasses should override for custom deserialization.
   */
  async load(key: string): Promise<T | null> {
    if (!this.storageAdapter?.load) return null;
    const data = await this.storageAdapter.load(key);
    if (!data) return null;
    return JSON.parse(new TextDecoder().decode(data)) as T;
  }

  /**
   * Remove an item from storage.
   */
  async removeFromStorage(key: string): Promise<void> {
    if (!this.storageAdapter?.delete) return;
    await this.storageAdapter.delete(key);
  }

  /**
   * Initialize registry from storage (load all items).
   * Subclasses should override for custom logic.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter?.list) return;
    const ids = await this.storageAdapter.list();
    for (const id of ids) {
      const item = await this.load(id);
      if (item) {
        this.set(id, item);
      }
    }
  }
}
