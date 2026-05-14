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
    // Primitive types and null/undefined
    if (a === b) return true;
    if (a == null || b == null) return false;
    if (typeof a !== typeof b) return false;

    // Arrays
    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      return a.every((item, index) => this.deepEqual(item, b[index]));
    }

    // Objects
    if (typeof a === "object" && typeof b === "object") {
      const keysA = Object.keys(a);
      const keysB = Object.keys(b);

      if (keysA.length !== keysB.length) return false;

      return keysA.every(key => 
        keysB.includes(key) && this.deepEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key])
      );
    }

    // Other types (functions, symbols, etc.)
    return false;
  }
}
