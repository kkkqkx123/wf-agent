/**
 * Generic Delta Calculator
 *
 * Provides generic difference calculation logic that can be reused by Graph and Agent modules.
 */

/**
 * Delta calculation context
 * Provides additional information for delta calculation
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
  /** Whether to enable strict deep comparison (recursive); when disabled, uses JSON serialization */
  strictCompare?: boolean;
  /** Fields to ignore */
  ignoreFields?: string[];
}

/**
 * Generic Delta Calculator
 *
 * @template TSnapshot The type of snapshot
 * @template TDelta The type of delta
 */
export abstract class DeltaCalculator<TSnapshot, TDelta> {
  protected options: DeltaCalculatorOptions;

  constructor(options: DeltaCalculatorOptions = {}) {
    this.options = {
      strictCompare: true,
      ignoreFields: [],
      ...options,
    };
  }

  /**
   * Calculate the delta between two snapshots
   *
   * @param previous Previous snapshot
   * @param current Current snapshot
   * @param context Additional context for calculation (optional)
   * @returns Delta data
   */
  abstract calculateDelta(
    previous: TSnapshot,
    current: TSnapshot,
    context?: DeltaCalculatorContext,
  ): TDelta;

  /**
   * Calculate newly appended array elements (only detects additions at the end)
   *
   * @param previous Previous array
   * @param current Current array
   * @param keyExtractor Optional key extractor for deduplication
   * @returns Newly added elements
   */
  protected calculateAppendedDelta<T>(
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

    if (current.length > previous.length) {
      return current.slice(previous.length);
    }

    return [];
  }

  /**
   * Calculate object difference
   *
   * @param previous Previous object
   * @param current Current object
   * @returns Difference result
   */
  protected calculateObjectDelta<T extends Record<string, unknown>>(
    previous: T,
    current: T,
  ): {
    added: string[];
    modified: Map<string, { from: unknown; to: unknown }>;
    removed: string[];
  } {
    const added: string[] = [];
    const modified = new Map<string, { from: unknown; to: unknown }>();
    const removed: string[] = [];

    const ignoreFields = new Set(this.options.ignoreFields || []);

    for (const key of Object.keys(current)) {
      if (ignoreFields.has(key)) continue;

      if (!(key in previous)) {
        added.push(key);
      } else if (!this.isEqual(previous[key], current[key])) {
        modified.set(key, { from: previous[key], to: current[key] });
      }
    }

    for (const key of Object.keys(previous)) {
      if (ignoreFields.has(key)) continue;

      if (!(key in current)) {
        removed.push(key);
      }
    }

    return { added, modified, removed };
  }

  /**
   * Compare whether two values are equal
   */
  protected isEqual(a: unknown, b: unknown): boolean {
    if (a === b) return true;
    if (a == null || b == null) return a === b;
    if (typeof a !== typeof b) return false;

    if (!this.options.strictCompare) {
      return JSON.stringify(a) === JSON.stringify(b);
    }

    if (typeof a === "object") {
      if (a instanceof Date && b instanceof Date) {
        return a.getTime() === b.getTime();
      }

      if (Array.isArray(a) && Array.isArray(b)) {
        if (a.length !== b.length) return false;
        return a.every((item, index) => this.isEqual(item, b[index]));
      }

      if (a instanceof Map && b instanceof Map) {
        if (a.size !== b.size) return false;
        for (const [key, value] of a) {
          if (!b.has(key) || !this.isEqual(value, b.get(key))) {
            return false;
          }
        }
        return true;
      }

      if (a instanceof Set && b instanceof Set) {
        if (a.size !== b.size) return false;
        for (const value of a) {
          if (!b.has(value)) return false;
        }
        return true;
      }

      const keysA = Object.keys(a as object);
      const keysB = Object.keys(b as object);
      if (keysA.length !== keysB.length) return false;

      for (const key of keysA) {
        if (!keysB.includes(key)) return false;
        if (!this.isEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]))
          return false;
      }
      return true;
    }

    return false;
  }
}
