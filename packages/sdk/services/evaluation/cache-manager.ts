/**
 * Cache Manager
 * Unified caching system for all condition types
 * Manages compilation cache, execution results, and dependency tracking
 *
 * Features:
 * - LRU (Least Recently Used) eviction policy for both caches
 * - Automatic dependency change detection via deep equality checks
 * - Circular reference protection in equality comparisons
 * - Comprehensive cache hit rate statistics
 * - Error handling for invalid context paths
 * - Compiler-independent cache key generation
 *
 * Design:
 * Uses the generic CacheManager from @wf-agent/common-utils/cache as internal
 * storage for both compilation and execution caches, avoiding direct coupling
 * to lru-cache and eliminating duplicate LRU/hash logic.
 *
 * Thread Safety:
 * This class is designed for single-threaded (Node.js main thread) usage.
 * It is NOT thread-safe for use with Worker threads.
 * If using Worker threads, consider using worker-specific cache instances or external synchronization.
 */

import type { EvaluationContext } from "@wf-agent/types";
import type { CompiledUnit } from "./types/index.js";
import { resolveContextPath } from "@sdk/services/evaluation/shared/path-resolver.js";
import { getGlobalLogger } from "@wf-agent/common-utils";
import { createHashAlgorithm, type IHashAlgorithm } from "@wf-agent/common-utils/cache";
import { CacheManager as CommonCacheManager, createCacheSync } from "@wf-agent/common-utils/cache";

interface CachedResult {
  result: unknown;
  dependencies: string[];
  timestamp: number;
  previousValues: Map<string, unknown>;
  useShallowComparison: boolean;
}

interface CacheManagerOptions {
  /**
   * Use shallow comparison for dependency change detection.
   * Faster but may miss deep property changes.
   * Useful for large objects where deep comparison is expensive.
   */
  useShallowComparison?: boolean;
  /**
   * Maximum size of compilation cache
   */
  compilationCacheSize?: number;
  /**
   * Maximum size of execution cache
   */
  executionCacheSize?: number;
}

export class CacheManager {
  private logger = getGlobalLogger().child("CacheManager", { pkg: "sdk/workflow" });

  private hashAlgorithm: IHashAlgorithm;
  private initialized = false;

  // Compilation cache: uses CommonCacheManager (LRU) internally
  private compilationCache: CommonCacheManager<string, CompiledUnit>;

  // Execution cache: uses CommonCacheManager (LRU) internally
  private executionCache: CommonCacheManager<string, CachedResult>;

  private useShallowComparison: boolean;

  constructor(options?: CacheManagerOptions) {
    const maxCompilation = options?.compilationCacheSize ?? 1000;
    const maxExecution = options?.executionCacheSize ?? 5000;

    this.compilationCache = createCacheSync<string, CompiledUnit>({
      maxSize: maxCompilation,
      hashBits: 64,
      enableStats: true,
    });
    this.executionCache = createCacheSync<string, CachedResult>({
      maxSize: maxExecution,
      hashBits: 64,
      enableStats: true,
    });
    this.useShallowComparison = options?.useShallowComparison ?? false;

    // Initialize hash algorithm (xxHash64 - 10,000x faster than FNV-1a)
    this.hashAlgorithm = createHashAlgorithm("xxhash64");
    this.initializeInternal();
  }

  /**
   * Initialize hash algorithm and internal caches asynchronously
   */
  private async initializeInternal(): Promise<void> {
    if (this.initialized) return;

    await Promise.all([
      this.compilationCache.initialize(),
      this.executionCache.initialize(),
      this.hashAlgorithm.initialize?.(),
    ]);
    this.initialized = true;
  }

  /**
   * Generate a stable cache key for a compiled unit
   * Handles different input types (string for expression, object for others)
   * Uses xxHash64 for efficient key generation (10,000x faster than FNV-1a)
   */
  generateCompilationCacheKey(type: string, input: string | Record<string, unknown>): string {
    switch (type) {
      case "expression":
        return `expr:${this.hashValue(input as string)}`;

      case "predicate": {
        const pred = input as Record<string, unknown>;
        return `pred:${pred["predicateType"]}:${this.hashValue(pred["variable"])}`;
      }

      case "schema": {
        const schema = input as Record<string, unknown>;
        const variable = schema["variable"] as string ?? "";
        const schemaHash = this.hashValue(schema["schema"]);
        return `schema:${variable}:${schemaHash}`;
      }

      case "script":
        return `script:${this.hashValue(input as string)}`;

      default:
        return `unknown:${this.hashValue(input)}`;
    }
  }

  /**
   * Hash a value using xxHash64
   * Replaces previous FNV-1a implementation (35 lines removed)
   */
  private hashValue(input: unknown): string {
    // If the hash algorithm is not yet initialized, fall back to JSON.stringify
    // to avoid throwing during early construction.
    if (!this.hashAlgorithm.isInitialized?.()) {
      return JSON.stringify(input);
    }
    return this.hashAlgorithm.hash(input);
  }

  /**
   * Initialize the cache manager's hash algorithm and internal caches.
   * Should be called during application startup.
   */
  async initialize(): Promise<void> {
    await this.initializeInternal();
  }

  /**
   * Get compiled unit from cache
   */
  getCompiled(cacheKey: string): CompiledUnit | null {
    if (!this.initialized) return null;
    try {
      return this.compilationCache.get(cacheKey) ?? null;
    } catch {
      return null;
    }
  }

  /**
   * Store compiled unit in cache
   */
  setCompiled(cacheKey: string, unit: CompiledUnit): void {
    if (!this.initialized) return;
    try {
      this.compilationCache.set(cacheKey, unit);
    } catch {
      // Cache not ready yet, skip
    }
  }

  /**
   * Get cached execution result
   */
  getCachedResult(cacheKey: string): unknown | null {
    if (!this.initialized) return null;
    try {
      const cached = this.executionCache.get(cacheKey);
      if (cached) {
        return cached.result;
      }
      return null;
    } catch {
      return null;
    }
  }

  /**
   * Check if dependencies have changed since last execution
   */
  hasDependenciesChanged(cacheKey: string, context: EvaluationContext): boolean {
    if (!this.initialized) return true;
    try {
      const cached = this.executionCache.get(cacheKey);
      if (!cached) return true; // No cache, treat as changed

      const compareFunc = cached.useShallowComparison ? this.valuesShallowEqual : this.valuesEqual;

      for (const dep of cached.dependencies) {
        const currentValue = this.getContextValue(dep, context);
        const previousValue = cached.previousValues.get(dep);

        if (!compareFunc.call(this, previousValue, currentValue)) {
          return true;
        }
      }

      return false;
    } catch {
      return true;
    }
  }

  /**
   * Store execution result with dependency information
   */
  setCachedResult(
    cacheKey: string,
    result: unknown,
    dependencies: string[],
    context: EvaluationContext,
  ): void {
    if (!this.initialized) return;
    try {
      const previousValues = new Map<string, unknown>();
      for (const dep of dependencies) {
        previousValues.set(dep, this.getContextValue(dep, context));
      }

      this.executionCache.set(cacheKey, {
        result,
        dependencies,
        timestamp: Date.now(),
        previousValues,
        useShallowComparison: this.useShallowComparison,
      });
    } catch {
      // Cache not ready yet, skip
    }
  }

  /**
   * Clear all caches
   */
  clear(): void {
    if (!this.initialized) return;
    this.compilationCache.clear();
    this.executionCache.clear();
    this.logger.debug("Cache cleared");
  }

  /**
   * Clear execution cache only (keep compilation cache)
   */
  clearExecutionCache(): void {
    if (!this.initialized) return;
    this.executionCache.clear();
  }

  /**
   * Get cache statistics
   */
  getStats(): {
    compilation: number;
    execution: number;
    compilationHits: number;
    compilationMisses: number;
    compilationHitRate: number;
    executionHits: number;
    executionMisses: number;
    executionHitRate: number;
  } {
    if (!this.initialized) {
      return {
        compilation: 0,
        execution: 0,
        compilationHits: 0,
        compilationMisses: 0,
        compilationHitRate: 0,
        executionHits: 0,
        executionMisses: 0,
        executionHitRate: 0,
      };
    }

    const compStats = this.compilationCache.getStats();
    const execStats = this.executionCache.getStats();

    return {
      compilation: compStats.size,
      execution: execStats.size,
      compilationHits: compStats.hits,
      compilationMisses: compStats.misses,
      compilationHitRate: compStats.hitRate,
      executionHits: execStats.hits,
      executionMisses: execStats.misses,
      executionHitRate: execStats.hitRate,
    };
  }

  /**
   * Extract value from context by dependency path
   */
  private getContextValue(dep: string, context: EvaluationContext): unknown {
    try {
      return resolveContextPath(dep, context);
    } catch (error) {
      this.logger.warn(`Failed to resolve context path: ${dep}`, { error });
      return undefined;
    }
  }

  /**
   * Shallow equality check (reference comparison for objects)
   * Faster than deep equality, suitable for large objects where identity matters
   */
  private valuesShallowEqual(a: unknown, b: unknown): boolean {
    return a === b;
  }

  /**
   * Deep equality check with circular reference protection
   */
  private valuesEqual(a: unknown, b: unknown, visited = new WeakSet()): boolean {
    if (a === b) return true;
    if (a == null || b == null) return a === b;
    if (typeof a !== typeof b) return false;

    // Circular reference protection for objects
    if (typeof a === "object") {
      if (visited.has(a as object)) return true; // Already checking this object
      visited.add(a as object);
    }

    if (typeof a === "object" && !Array.isArray(a)) {
      const aObj = a as Record<string, unknown>;
      const bObj = b as Record<string, unknown>;
      const keysA = Object.keys(aObj);
      const keysB = Object.keys(bObj);

      if (keysA.length !== keysB.length) return false;
      return keysA.every(key => this.valuesEqual(aObj[key], bObj[key], visited));
    }

    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      return a.every((item, idx) => this.valuesEqual(item, b[idx], visited));
    }

    return false;
  }
}

/**
 * Singleton cache manager instance.
 * Note: This is created at module load time but internal caches are NOT
 * initialized until initialize() is explicitly called during application startup.
 * Before that, all get/set operations are safe no-ops returning null.
 */
export const cacheManager = new CacheManager();
export type { CacheManagerOptions };