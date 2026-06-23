/**
 * Registry Internals
 *
 * Provides factory functions for creating type-safe in-memory registries.
 * Uses composition over inheritance for better flexibility and simplicity.
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
