/**
 * Base Diff Calculator
 * 
 * Provides generic deep comparison and delta calculation algorithms.
 * Works with any state snapshot type through generic typing.
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BaseDiffCalculator" });

/**
 * Base Diff Calculator
 * 
 * Provides generic deep comparison and delta calculation algorithms.
 * Works with any state snapshot type through generic typing.
 */
export class BaseDiffCalculator {
  /**
   * Calculate delta between two state snapshots
   * @param previous Previous state
   * @param current Current state
   * @returns Delta object containing only changed fields
   */
  calculateDelta<T extends Record<string, unknown>>(
    previous: T,
    current: T
  ): Record<string, { from: unknown; to: unknown }> {
    const delta: Record<string, { from: unknown; to: unknown }> = {};

    // Compare all keys in current state
    for (const key of Object.keys(current)) {
      const prevValue = previous[key];
      const currValue = current[key];

      if (!this.deepEqual(prevValue, currValue)) {
        delta[key] = {
          from: prevValue,
          to: currValue,
        };
      }
    }

    // Check for deleted keys
    for (const key of Object.keys(previous)) {
      if (!(key in current)) {
        delta[key] = {
          from: previous[key],
          to: undefined,
        };
      }
    }

    logger.debug("Delta calculated", {
      changedFields: Object.keys(delta).length,
      totalFields: Object.keys(current).length,
    });

    return delta;
  }

  /**
   * Apply delta to a state snapshot
   * @param base Base state snapshot
   * @param delta Delta to apply
   * @returns New state snapshot with delta applied
   */
  applyDelta<T extends Record<string, unknown>>(
    base: T,
    delta: Record<string, { from: unknown; to: unknown }>
  ): T {
    const result = { ...base } as T;

    for (const [key, change] of Object.entries(delta)) {
      if (change.to === undefined) {
        // Delete field
        delete (result as Record<string, unknown>)[key];
      } else {
        // Update field
        (result as Record<string, unknown>)[key] = change.to;
      }
    }

    return result;
  }

  /**
   * Deep equality check
   * @param a First value
   * @param b Second value
   * @returns True if values are deeply equal
   */
  private deepEqual(a: unknown, b: unknown): boolean {
    // Strict equality (handles NaN correctly via Object.is)
    if (Object.is(a, b)) return true;
    // One is null/undefined but not the same value (already filtered by Object.is)
    if (a == null || b == null) return false;
    // Different types
    if (typeof a !== typeof b) return false;

    // Date
    if (a instanceof Date && b instanceof Date) {
      return a.getTime() === b.getTime();
    }

    // RegExp
    if (a instanceof RegExp && b instanceof RegExp) {
      return a.source === b.source && a.flags === b.flags;
    }

    // Map
    if (a instanceof Map && b instanceof Map) {
      if (a.size !== b.size) return false;
      for (const [key, val] of a) {
        if (!b.has(key) || !this.deepEqual(val, b.get(key))) return false;
      }
      return true;
    }

    // Set
    if (a instanceof Set && b instanceof Set) {
      if (a.size !== b.size) return false;
      // Set order is not significant, compare by sorted contents
      const itemsA = [...a].sort();
      const itemsB = [...b].sort();
      return itemsA.every((item, i) => this.deepEqual(item, itemsB[i]));
    }

    // Uint8Array / Buffer
    if (a instanceof Uint8Array && b instanceof Uint8Array) {
      return a.length === b.length && a.every((val, i) => val === b[i]);
    }

    // Arrays
    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      return a.every((item, index) => this.deepEqual(item, b[index]));
    }

    // Plain objects
    if (typeof a === "object" && typeof b === "object" &&
        a.constructor === Object && b.constructor === Object) {
      const keysA = Object.keys(a);
      const keysB = Object.keys(b);

      if (keysA.length !== keysB.length) return false;

      return keysA.every(key =>
        keysB.includes(key) && this.deepEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key])
      );
    }

    // Other types (class instances, functions, symbols, etc.) — referential equality already checked
    return false;
  }
}
