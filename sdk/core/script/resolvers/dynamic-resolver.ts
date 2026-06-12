/**
 * Dynamic Resolver
 * Resolves dynamic variable references and expressions at runtime
 */

import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "DynamicResolver" });

/**
 * Dynamic Resolver
 * Resolves variable references ($variable) and expressions within argument values
 */
export class DynamicResolver {
  /**
   * Resolve dynamic references within a value
   * @param value The value which may contain $variable references
   * @param context Current execution context variables
   * @returns Resolved value with all references replaced
   */
  resolve(value: unknown, context: Record<string, unknown>): unknown {
    if (typeof value === "string") {
      return this.resolveString(value, context);
    }
    if (Array.isArray(value)) {
      return value.map(item => this.resolve(item, context));
    }
    if (value !== null && typeof value === "object") {
      const resolved: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
        resolved[k] = this.resolve(v, context);
      }
      return resolved;
    }
    return value;
  }

  /**
   * Resolve references within a string
   */
  private resolveString(value: string, context: Record<string, unknown>): string {
    return value.replace(/\$(\w+(?:\.\w+)*)/g, (match, refPath: string) => {
      const resolved = this.resolvePath(refPath, context);
      if (resolved === undefined) {
        logger.warn("Unresolved dynamic reference", { ref: refPath });
        return match;
      }
      return String(resolved);
    });
  }

  /**
   * Resolve a dotted path against context
   */
  private resolvePath(path: string, context: Record<string, unknown>): unknown {
    const parts = path.split(".");
    let current: unknown = context;

    for (const part of parts) {
      if (current === null || current === undefined) {
        return undefined;
      }
      if (typeof current === "object") {
        current = (current as Record<string, unknown>)[part];
      } else {
        return undefined;
      }
    }

    return current;
  }
}
