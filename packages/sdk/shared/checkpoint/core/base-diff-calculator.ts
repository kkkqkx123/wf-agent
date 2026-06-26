import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BaseDiffCalculator" });

export type DeltaMap = Record<string, { from: unknown; to: unknown }>;

export class BaseDiffCalculator {
  calculateDelta<T extends Record<string, unknown>>(
    previous: T,
    current: T,
  ): Record<string, { from: unknown; to: unknown }> {
    const delta: Record<string, { from: unknown; to: unknown }> = {};

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

  applyDelta<T extends Record<string, unknown>>(
    base: T,
    delta: Record<string, { from: unknown; to: unknown }>,
  ): T {
    const result = this.deepClone(base) as T;

    for (const [key, change] of Object.entries(delta)) {
      if (change.to === undefined) {
        delete (result as Record<string, unknown>)[key];
      } else {
        (result as Record<string, unknown>)[key] = this.deepClone(change.to);
      }
    }

    return result;
  }

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
        from: firstFrom ?? change.from,
        to: change.to,
      };
    }

    return merged;
  }

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

  private deepEqual(a: unknown, b: unknown): boolean {
    if (Object.is(a, b)) return true;
    if (a == null || b == null) return false;
    if (typeof a !== typeof b) return false;

    if (a instanceof Date && b instanceof Date) {
      return a.getTime() === b.getTime();
    }

    if (a instanceof RegExp && b instanceof RegExp) {
      return a.source === b.source && a.flags === b.flags;
    }

    if (a instanceof Map && b instanceof Map) {
      if (a.size !== b.size) return false;
      for (const [key, val] of a) {
        if (!b.has(key) || !this.deepEqual(val, b.get(key))) return false;
      }
      return true;
    }

     if (a instanceof Set && b instanceof Set) {
       if (a.size !== b.size) return false;
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

    if (ArrayBuffer.isView(a) && ArrayBuffer.isView(b)) {
      if (a.byteLength !== b.byteLength) return false;
      const aView = new Uint8Array(a.buffer, a.byteOffset, a.byteLength);
      const bView = new Uint8Array(b.buffer, b.byteOffset, b.byteLength);
      return aView.every((val, i) => val === bView[i]);
    }

    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      return a.every((item, index) => this.deepEqual(item, b[index]));
    }

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

    return false;
  }
}
