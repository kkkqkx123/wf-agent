/**
 * Generic Delta Calculator
 *
 * Provides generic difference calculation logic for snapshots.
 * Used by both Graph and Agent modules.
 */

import type { SnapshotBase, DeltaResult } from "@wf-agent/types";

/**
 * Delta calculation context
 */
export interface DeltaCalculatorContext {
  /** Previous message count (for message optimization) */
  previousMessageCount?: number;
  /** Current messages */
  currentMessages?: unknown[];
  /** Additional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Delta calculation options
 */
export interface DeltaCalculatorOptions {
  /** Whether to enable deep comparison */
  deepCompare?: boolean;
  /** Fields to ignore during comparison */
  ignoreFields?: string[];
  /** Key extractor for array items */
  arrayKeyExtractor?: (item: unknown) => string;
}

/**
 * Default delta calculation options
 */
const DEFAULT_OPTIONS: DeltaCalculatorOptions = {
  deepCompare: true,
  ignoreFields: ["_timestamp"],
};

/**
 * Generic Delta Calculator
 *
 * @template TSnapshot The snapshot type
 */
export class DeltaCalculator<TSnapshot extends SnapshotBase> {
  protected readonly options: DeltaCalculatorOptions;

  constructor(options?: DeltaCalculatorOptions) {
    this.options = { ...DEFAULT_OPTIONS, ...options };
  }

  /**
   * Calculate the delta between two snapshots
   *
   * @param previous Previous snapshot (null for full snapshot)
   * @param current Current snapshot
   * @param context Additional context for calculation
   * @returns Delta result
   */
  calculate(
    previous: TSnapshot | null,
    current: TSnapshot,
    context?: DeltaCalculatorContext,
  ): DeltaResult<TSnapshot> {
    if (!previous) {
      return {
        type: "FULL",
        snapshot: current,
      };
    }

    const delta = this.computeDelta(previous, current, context);

    if (Object.keys(delta).length === 0) {
      return {
        type: "FULL",
        snapshot: current,
      };
    }

    return {
      type: "DELTA",
      delta,
      baseSnapshotId: previous._entityType,
    };
  }

  /**
   * Compute the delta between two snapshots
   *
   * @param previous Previous snapshot
   * @param current Current snapshot
   * @param context Additional context
   * @returns Partial snapshot containing only changed fields
   */
  protected computeDelta(
    previous: TSnapshot,
    current: TSnapshot,
    _context?: DeltaCalculatorContext,
  ): Partial<TSnapshot> {
    const delta: Partial<TSnapshot> = {};
    const keys = Object.keys(current) as (keyof TSnapshot)[];
    const ignoreFields = this.options.ignoreFields ?? [];

    for (const key of keys) {
      if (ignoreFields.includes(key as string)) {
        continue;
      }

      if (!this.deepEqual(previous[key], current[key])) {
        delta[key] = current[key];
      }
    }

    return delta;
  }

  /**
   * Calculate array difference (return only newly added elements)
   *
   * @param previous Previous array
   * @param current Current array
   * @param keyExtractor Optional key extractor for deduplication
   * @returns Newly added elements
   */
  protected calculateArrayDelta<T>(
    previous: T[],
    current: T[],
    keyExtractor?: (item: T) => string,
  ): T[] {
    if (!previous || previous.length === 0) {
      return [...current];
    }

    if (!current || current.length === 0) {
      return [];
    }

    if (keyExtractor) {
      const previousKeys = new Set(previous.map(keyExtractor));
      return current.filter(item => !previousKeys.has(keyExtractor(item)));
    }

    const previousSet = new Set(previous);
    return current.filter(item => !previousSet.has(item));
  }

  /**
   * Deep equality check
   */
  protected deepEqual(a: unknown, b: unknown): boolean {
    if (a === b) {
      return true;
    }

    if (a === null || b === null) {
      return a === b;
    }

    if (typeof a !== typeof b) {
      return false;
    }

    if (typeof a !== "object") {
      return a === b;
    }

    if (Array.isArray(a) !== Array.isArray(b)) {
      return false;
    }

    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) {
        return false;
      }
      return a.every((item, index) => this.deepEqual(item, b[index]));
    }

    const objA = a as Record<string, unknown>;
    const objB = b as Record<string, unknown>;
    const keysA = Object.keys(objA);
    const keysB = Object.keys(objB);

    if (keysA.length !== keysB.length) {
      return false;
    }

    return keysA.every(key => this.deepEqual(objA[key], objB[key]));
  }
}
