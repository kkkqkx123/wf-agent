/**
 * Predicate Compiler
 * Compiles predicate conditions (isEmpty, isNull, etc.)
 * Note: Caching is handled by CacheManager, not by this class
 */

import type { ICompiler, CompiledUnit } from "../types/index.js";

interface PredicateInput {
  type: string;
  variable: string;
}

export class PredicateCompiler implements ICompiler {
  compile(input: string | Record<string, unknown>): CompiledUnit {
    if (typeof input === "string") {
      throw new Error("Predicate compiler expects object input");
    }

    const config = input as unknown as PredicateInput;

    const unit: CompiledUnit = {
      ast: {
        type: config.type,
        variable: config.variable,
      },
      dependencies: [config.variable],
      complexity: 1,
      metadata: {
        type: "predicate",
        predicateType: config.type,
        variable: config.variable,
      },
    };

    return unit;
  }

  clearCache(): void {
    // Caching is handled by CacheManager, nothing to clear here
  }

  getCacheSize(): number {
    // Caching is handled by CacheManager
    return 0;
  }
}

export const predicateCompiler = new PredicateCompiler();
