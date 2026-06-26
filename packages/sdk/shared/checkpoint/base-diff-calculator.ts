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
export type DeltaMap = Record<string, { from: unknown; to: unknown }>;

export class BaseDiffCalculator {
  /**
   * Calculate delta between two state snapshots
   * @param previous Previous state
   * @param current Current state
   * @returns Delta object containing only changed fields
   */
  calculateDelta<T extends Record<string, unknown>>(
    previous: T,
    current: T,
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
    delta: Record<string, { from: unknown; to: unknown }>,
  ): T {
    const result = this.deepClone(base) as T;

    for (const [key, change] of Object.entries(delta)) {
      if (change.to === undefined) {
        // Delete field
        delete (result as Record<string, unknown>)[key];
      } else {
        // Update field with deep clone to preserve Map/Set types
        (result as Record<string, unknown>)[key] = this.deepClone(change.to);
      }
    }

    return result;
  }

  /**
   * Merge two consecutive deltas into a single delta
   *
   * Given delta1 representing state S0 → S1 and delta2 representing S1 → S2,
   * returns a delta representing S0 → S2 directly.
   *
   * The `from` field is informational only (not used by applyDelta).
   * For keys only in delta1, the value is preserved as-is.
   * For keys in delta2, the value overwrites delta1's value for that key.
   *
   * @param first First delta (earlier in chain)
   * @param second Second delta (later in chain, merged into first)
   * @returns Merged delta
   */
  mergeDeltas(
    first: DeltaMap,
    second: DeltaMap,
  ): DeltaMap {
    if (Object.keys(first).length === 0) return { ...second };
    if (Object.keys(second).length === 0) return { ...first };

    const merged: DeltaMap = { ...first };

    for (const [key, change] of Object.entries(second)) {
      const firstFrom = (merged[key] as { from: unknown } | undefined)?.from;
      merged[key] = {
        from: firstFrom ?? undefined,
        to: change.to,
      };
    }

    return merged;
  }

  /**
   * Deep clone a value, preserving Map and Set types
   * @param value Value to clone
   * @returns Deep cloned value
   */
  private deepClone(value: unknown): unknown {
    if (value === null || typeof value !== "object") {
      return value;
    }

    if (value instanceof Date) {
      return new Date(value.getTime());
    }

    if (value instanceof RegExp) {
      return new RegExp(value.source, value.flags);
    }

    if (value instanceof Map) {
      const cloned = new Map();
      for (const [k, v] of value) {
        cloned.set(k, this.deepClone(v));
      }
      return cloned;
    }

    if (value instanceof Set) {
      const cloned = new Set();
      for (const item of value) {
        cloned.add(this.deepClone(item));
      }
      return cloned;
    }

    if (Array.isArray(value)) {
      return value.map(item => this.deepClone(item));
    }

    if (ArrayBuffer.isView(value)) {
      const view = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
      return new Uint8Array(view);
    }

    if (value.constructor === Object) {
      const cloned: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(value)) {
        cloned[k] = this.deepClone(v);
      }
      return cloned;
    }

    return value;
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
       // Set order is not significant, compare by deep equality for each element
       // Note: b.has(item) uses referential equality which is incorrect for objects,
       // so we always use deep comparison for all elements
       for (const item of a) {
         let found = false;
         for (const bItem of b) {
           if (this.deepEqual(item, bItem)) {
             found = true;
             break;
           }
         }
         if (!found) return false;
       }
       return true;
     }

    // Typed arrays (Uint8Array, Int32Array, Float32Array, etc.)
    if (ArrayBuffer.isView(a) && ArrayBuffer.isView(b)) {
      if (a.byteLength !== b.byteLength) return false;
      const aView = new Uint8Array(a.buffer, a.byteOffset, a.byteLength);
      const bView = new Uint8Array(b.buffer, b.byteOffset, b.byteLength);
      return aView.every((val, i) => val === bView[i]);
    }

    // Arrays
    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      return a.every((item, index) => this.deepEqual(item, b[index]));
    }

    // Plain objects
    if (
      typeof a === "object" &&
      typeof b === "object" &&
      a.constructor === Object &&
      b.constructor === Object
    ) {
      const keysA = Object.keys(a);
      const keysB = Object.keys(b);

      if (keysA.length !== keysB.length) return false;

      return keysA.every(
        key =>
          keysB.includes(key) &&
          this.deepEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]),
      );
    }

    // Other types (class instances, functions, symbols, etc.) — referential equality already checked
    return false;
  }
}
