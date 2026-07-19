/**
 * CacheManager Tests
 * Tests compilation and execution caching with dependency tracking
 */

import { describe, it, expect, beforeEach } from "vitest";
import { CacheManager } from "../cache-manager.js";
import type { CompiledUnit } from "../types/index.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("CacheManager", () => {
  let cache: CacheManager;

  const mockCompiledUnit = (deps: string[] = []): CompiledUnit => ({
    ast: { type: "test" },
    dependencies: deps,
    complexity: 1,
    metadata: { type: "test" }
  });

  const mockContext = (vars: Record<string, unknown> = {}): EvaluationContext => ({
    variables: vars,
    input: { userId: 1, username: "test" },
    output: { status: "success", data: {} }
  });

  beforeEach(async () => {
    cache = new CacheManager();
    // Initialize hash algorithm for xxHash64
    await cache.initialize();
  });

  describe("Compilation Cache", () => {
    it("should store compiled units", () => {
      const key = "expr:x>5";
      const unit = mockCompiledUnit();
      cache.setCompiled(key, unit);

      expect(cache.getCompiled(key)).toBe(unit);
    });

    it("should return null for non-existing compiled units", () => {
      expect(cache.getCompiled("nonexistent")).toBeNull();
    });

    it("should maintain compilation cache across multiple retrievals", () => {
      const key = "expr:complex";
      const unit = mockCompiledUnit();
      cache.setCompiled(key, unit);
      cache.setCompiled(key, unit);

      expect(cache.getCompiled(key)).toBe(unit);
    });

    it("should clear compilation cache", () => {
      cache.setCompiled("key1", mockCompiledUnit());
      cache.setCompiled("key2", mockCompiledUnit());
      cache.clear();

      expect(cache.getCompiled("key1")).toBeNull();
      expect(cache.getCompiled("key2")).toBeNull();
    });

    it("should report correct cache size", () => {
      const stats = cache.getStats();
      expect(stats.compilation).toBe(0);
      expect(stats.execution).toBe(0);

      cache.setCompiled("key1", mockCompiledUnit());
      cache.setCompiled("key2", mockCompiledUnit());

      const updatedStats = cache.getStats();
      expect(updatedStats.compilation).toBe(2);
    });
  });

  describe("Execution Result Cache", () => {
    it("should store execution results with dependencies", () => {
      const key = "result:1";
      const context = mockContext({ x: 5 });
      cache.setCachedResult(key, true, ["x"], context);

      expect(cache.getCachedResult(key)).toBe(true);
    });

    it("should cache boolean results", () => {
      const context = mockContext({ status: "active" });
      cache.setCachedResult("cond:1", true, ["status"], context);
      cache.setCachedResult("cond:2", false, ["status"], context);

      expect(cache.getCachedResult("cond:1")).toBe(true);
      expect(cache.getCachedResult("cond:2")).toBe(false);
    });

    it("should cache complex values", () => {
      const value = { result: "success", data: [1, 2, 3] };
      const context = mockContext({});
      cache.setCachedResult("key", value, [], context);

      expect(cache.getCachedResult("key")).toEqual(value);
    });

    it("should return null for non-cached results", () => {
      expect(cache.getCachedResult("nonexistent")).toBeNull();
    });

    it("should clear only execution cache", () => {
      cache.setCompiled("compiled", mockCompiledUnit());
      cache.setCachedResult("result", true, [], mockContext());

      cache.clearExecutionCache();

      expect(cache.getCompiled("compiled")).not.toBeNull();
      expect(cache.getCachedResult("result")).toBeNull();
    });
  });

  describe("Dependency Change Detection", () => {
    it("should detect when dependencies have not changed", () => {
      const context = mockContext({ x: 5, y: "hello" });
      cache.setCachedResult("key", true, ["x", "y"], context);

      const unchanged = cache.hasDependenciesChanged("key", context);
      expect(unchanged).toBe(false);
    });

    it("should detect when a dependency has changed", () => {
      let context = mockContext({ x: 5 });
      cache.setCachedResult("key", true, ["x"], context);

      const changedContext = mockContext({ x: 10 });
      const changed = cache.hasDependenciesChanged("key", changedContext);
      expect(changed).toBe(true);
    });

    it("should detect when dependency becomes null", () => {
      let context = mockContext({ x: 5 });
      cache.setCachedResult("key", true, ["x"], context);

      const nullContext = mockContext({});
      const changed = cache.hasDependenciesChanged("key", nullContext);
      expect(changed).toBe(true);
    });

    it("should handle nested dependency changes", () => {
      let context: EvaluationContext = {
        variables: { user: { role: "admin" } },
        input: {},
        output: {}
      };
      cache.setCachedResult("key", true, ["user"], context);

      const changedContext: EvaluationContext = {
        variables: { user: { role: "user" } },
        input: {},
        output: {}
      };
      const changed = cache.hasDependenciesChanged("key", changedContext);
      expect(changed).toBe(true);
    });

    it("should treat missing cache as changed", () => {
      const context = mockContext({ x: 5 });
      const changed = cache.hasDependenciesChanged("nonexistent", context);
      expect(changed).toBe(true);
    });
  });

  describe("Cache Key Generation (xxHash64)", () => {
    it("should generate consistent cache keys for expressions", () => {
      const key1 = cache.generateCompilationCacheKey("expression", "x > 5");
      const key2 = cache.generateCompilationCacheKey("expression", "x > 5");
      expect(key1).toBe(key2);
    });

    it("should generate different cache keys for different expressions", () => {
      const key1 = cache.generateCompilationCacheKey("expression", "x > 5");
      const key2 = cache.generateCompilationCacheKey("expression", "x > 10");
      expect(key1).not.toBe(key2);
    });

    it("should generate cache keys for predicates", () => {
      const predicate = { predicateType: "comparison", variable: "status" };
      const key = cache.generateCompilationCacheKey("predicate", predicate);
      expect(key).toContain("pred:");
      expect(key).toContain("comparison");
    });

    it("should generate cache keys for schemas", () => {
      const schema = { variable: "user", schema: { type: "object", properties: {} } };
      const key = cache.generateCompilationCacheKey("schema", schema);
      expect(key).toContain("schema:");
      expect(key).toContain("user");
    });

    it("should generate cache keys for scripts", () => {
      const key = cache.generateCompilationCacheKey("script", "return x + y;");
      expect(key).toContain("script:");
    });
  });

    it("should cache results with no dependencies", () => {
      const context = mockContext({});
      cache.setCachedResult("key", 42, [], context);
      expect(cache.getCachedResult("key")).toBe(42);
    });

    it("should never invalidate cache with no dependencies", () => {
      let context = mockContext({ x: 1 });
      cache.setCachedResult("key", true, [], context);

      const changedContext = mockContext({ x: 999 });
      expect(cache.hasDependenciesChanged("key", changedContext)).toBe(false);
    });

  describe("Cache Statistics", () => {
    it("should track cache sizes", () => {
      cache.setCompiled("key1", mockCompiledUnit());
      cache.setCompiled("key2", mockCompiledUnit());
      cache.setCachedResult("result1", true, [], mockContext());

      const stats = cache.getStats();
      expect(stats.compilation).toBe(2);
      expect(stats.execution).toBe(1);
    });

    it("should return zero stats for empty cache", () => {
      const stats = cache.getStats();
      expect(stats.compilation).toBe(0);
      expect(stats.execution).toBe(0);
    });

    it("should track stats after clear", () => {
      cache.setCompiled("key", mockCompiledUnit());
      cache.clear();

      const stats = cache.getStats();
      expect(stats.compilation).toBe(0);
    });

    it("should track compilation cache hit rate", () => {
      const unit = mockCompiledUnit();
      cache.setCompiled("key1", unit);

      cache.getCompiled("key1"); // hit
      cache.getCompiled("key1"); // hit
      cache.getCompiled("nonexistent"); // miss

      const stats = cache.getStats();
      expect(stats.compilationHits).toBe(2);
      expect(stats.compilationMisses).toBe(1);
      expect(stats.compilationHitRate).toBeCloseTo(2 / 3);
    });

    it("should track execution cache hit rate", () => {
      const context = mockContext({});
      cache.setCachedResult("result1", true, [], context);

      cache.getCachedResult("result1"); // hit
      cache.getCachedResult("result1"); // hit
      cache.getCachedResult("nonexistent"); // miss

      const stats = cache.getStats();
      expect(stats.executionHits).toBe(2);
      expect(stats.executionMisses).toBe(1);
      expect(stats.executionHitRate).toBeCloseTo(2 / 3);
    });

    it("should reset statistics on clear", () => {
      const unit = mockCompiledUnit();
      cache.setCompiled("key", unit);
      cache.getCompiled("key");

      cache.clear();

      const stats = cache.getStats();
      expect(stats.compilationHits).toBe(0);
      expect(stats.compilationMisses).toBe(0);
      expect(stats.compilationHitRate).toBe(0);
    });
  });

  describe("Value Equality Checking", () => {
    it("should detect identical primitive changes", () => {
      const context = mockContext({ x: 5 });
      cache.setCachedResult("key", true, ["x"], context);

      const sameContext = mockContext({ x: 5 });
      expect(cache.hasDependenciesChanged("key", sameContext)).toBe(false);

      const differentContext = mockContext({ x: 6 });
      expect(cache.hasDependenciesChanged("key", differentContext)).toBe(true);
    });

    it("should detect object changes through deep equality", () => {
      const context: EvaluationContext = {
        variables: { data: { a: 1, b: 2 } },
        input: {},
        output: {}
      };
      cache.setCachedResult("key", true, ["data"], context);

      const sameContext: EvaluationContext = {
        variables: { data: { a: 1, b: 2 } },
        input: {},
        output: {}
      };
      expect(cache.hasDependenciesChanged("key", sameContext)).toBe(false);

      const differentContext: EvaluationContext = {
        variables: { data: { a: 1, b: 3 } },
        input: {},
        output: {}
      };
      expect(cache.hasDependenciesChanged("key", differentContext)).toBe(true);
    });

    it("should detect array changes", () => {
      const context: EvaluationContext = {
        variables: { items: [1, 2, 3] },
        input: {},
        output: {}
      };
      cache.setCachedResult("key", true, ["items"], context);

      const differentContext: EvaluationContext = {
        variables: { items: [1, 2, 3, 4] },
        input: {},
        output: {}
      };
      expect(cache.hasDependenciesChanged("key", differentContext)).toBe(true);
    });

    it("should use shallow comparison when configured", () => {
      const shallowCache = new CacheManager({ useShallowComparison: true });
      const obj = { a: 1 };
      const context: EvaluationContext = {
        variables: { data: obj },
        input: {},
        output: {}
      };
      shallowCache.setCachedResult("key", true, ["data"], context);

      // Same object reference should not trigger change
      expect(shallowCache.hasDependenciesChanged("key", context)).toBe(false);

      // Different object (even with same content) should trigger change
      const differentContext: EvaluationContext = {
        variables: { data: { a: 1 } },
        input: {},
        output: {}
      };
      expect(shallowCache.hasDependenciesChanged("key", differentContext)).toBe(true);
    });
  });
});
