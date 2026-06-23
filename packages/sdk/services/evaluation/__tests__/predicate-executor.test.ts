/**
 * Predicate Executor & Compiler Tests
 * Tests predicate conditions (isEmpty, isNull, etc.)
 */

import { describe, it, expect } from "vitest";
import { predicateExecutor } from "../executors/predicate-executor.js";
import { predicateCompiler } from "../compilers/predicate-compiler.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("PredicateCompiler", () => {
  it("should compile isEmpty predicate", () => {
    const compiled = predicateCompiler.compile({
      type: "isEmpty",
      variable: "items"
    });

    expect(compiled.ast).toEqual({
      type: "isEmpty",
      variable: "items"
    });
    expect(compiled.dependencies).toEqual(["items"]);
    expect(compiled.complexity).toBe(1);
  });

  it("should compile isNotEmpty predicate", () => {
    const compiled = predicateCompiler.compile({
      type: "isNotEmpty",
      variable: "content"
    });

    expect(compiled.dependencies).toEqual(["content"]);
  });

  it("should compile isNull predicate", () => {
    const compiled = predicateCompiler.compile({
      type: "isNull",
      variable: "value"
    });

    expect(compiled.ast.type).toBe("isNull");
  });

  it("should compile isTrue predicate", () => {
    const compiled = predicateCompiler.compile({
      type: "isTrue",
      variable: "enabled"
    });

    expect(compiled.ast.type).toBe("isTrue");
  });

  it("should throw on string input", () => {
    expect(() => predicateCompiler.compile("invalid")).toThrow();
  });

  it("should produce consistent output for same input", () => {
    // Caching is now handled by CacheManager, not by compiler
    // Compiler just produces consistent CompiledUnit structure
    const input = { type: "isEmpty", variable: "x" };
    const compiled1 = predicateCompiler.compile(input);
    const compiled2 = predicateCompiler.compile(input);

    // Both should have the same structure and content
    expect(compiled1.ast).toStrictEqual(compiled2.ast);
    expect(compiled1.dependencies).toStrictEqual(compiled2.dependencies);
    expect(compiled1.complexity).toBe(compiled2.complexity);
  });

  it("should track cache size (delegated to CacheManager)", () => {
    // Compiler no longer has its own cache
    // getCacheSize() always returns 0
    predicateCompiler.clearCache();
    expect(predicateCompiler.getCacheSize()).toBe(0);

    predicateCompiler.compile({ type: "isEmpty", variable: "x" });
    // Still 0 because caching is handled by CacheManager
    expect(predicateCompiler.getCacheSize()).toBe(0);

    predicateCompiler.compile({ type: "isEmpty", variable: "y" });
    expect(predicateCompiler.getCacheSize()).toBe(0);
  });
});

describe("PredicateExecutor", () => {
  const makeContext = (vars: Record<string, unknown> = {}): EvaluationContext => ({
    variables: vars,
    input: {},
    output: {}
  });

  describe("isEmpty", () => {
    it("should return true for null", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "value"
      });
      const context = makeContext({ value: null });

      expect(predicateExecutor.execute(compiled, context)).toBe(true);
    });

    it("should return true for undefined", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "missing"
      });
      const context = makeContext({});

      expect(predicateExecutor.execute(compiled, context)).toBe(true);
    });

    it("should return true for empty string", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "text"
      });
      const context = makeContext({ text: "" });

      expect(predicateExecutor.execute(compiled, context)).toBe(true);
    });

    it("should return true for empty array", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "items"
      });
      const context = makeContext({ items: [] });

      expect(predicateExecutor.execute(compiled, context)).toBe(true);
    });

    it("should return true for empty object", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "data"
      });
      const context = makeContext({ data: {} });

      expect(predicateExecutor.execute(compiled, context)).toBe(true);
    });

    it("should return false for non-empty values", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "text"
      });

      expect(predicateExecutor.execute(compiled, makeContext({ text: "hello" }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ text: 0 }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ text: false }))).toBe(false);
    });

    it("should return false for non-empty array", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "items"
      });
      const context = makeContext({ items: [1, 2, 3] });

      expect(predicateExecutor.execute(compiled, context)).toBe(false);
    });

    it("should return false for non-empty object", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "data"
      });
      const context = makeContext({ data: { x: 1 } });

      expect(predicateExecutor.execute(compiled, context)).toBe(false);
    });
  });

  describe("isNotEmpty", () => {
    it("should return opposite of isEmpty", () => {
      const compiled = predicateCompiler.compile({
        type: "isNotEmpty",
        variable: "value"
      });

      expect(predicateExecutor.execute(compiled, makeContext({ value: null }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ value: "text" }))).toBe(true);
      expect(predicateExecutor.execute(compiled, makeContext({ value: [] }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ value: [1] }))).toBe(true);
    });
  });

  describe("isNull", () => {
    it("should return true only for null", () => {
      const compiled = predicateCompiler.compile({
        type: "isNull",
        variable: "value"
      });

      expect(predicateExecutor.execute(compiled, makeContext({ value: null }))).toBe(true);
      expect(predicateExecutor.execute(compiled, makeContext({ value: undefined }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ value: "" }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ value: 0 }))).toBe(false);
    });
  });

  describe("isNotNull", () => {
    it("should return true for all non-null values", () => {
      const compiled = predicateCompiler.compile({
        type: "isNotNull",
        variable: "value"
      });

      expect(predicateExecutor.execute(compiled, makeContext({ value: null }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ value: undefined }))).toBe(true);
      expect(predicateExecutor.execute(compiled, makeContext({ value: "" }))).toBe(true);
      expect(predicateExecutor.execute(compiled, makeContext({ value: 0 }))).toBe(true);
      expect(predicateExecutor.execute(compiled, makeContext({ value: false }))).toBe(true);
    });
  });

  describe("isTrue", () => {
    it("should return true only for true value", () => {
      const compiled = predicateCompiler.compile({
        type: "isTrue",
        variable: "flag"
      });

      expect(predicateExecutor.execute(compiled, makeContext({ flag: true }))).toBe(true);
      expect(predicateExecutor.execute(compiled, makeContext({ flag: false }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ flag: 1 }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ flag: null }))).toBe(false);
    });
  });

  describe("isFalse", () => {
    it("should return true only for false value", () => {
      const compiled = predicateCompiler.compile({
        type: "isFalse",
        variable: "flag"
      });

      expect(predicateExecutor.execute(compiled, makeContext({ flag: false }))).toBe(true);
      expect(predicateExecutor.execute(compiled, makeContext({ flag: true }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ flag: 0 }))).toBe(false);
      expect(predicateExecutor.execute(compiled, makeContext({ flag: null }))).toBe(false);
    });
  });

  describe("Context validation", () => {
    it("should throw on invalid context", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "x"
      });

      expect(() => predicateExecutor.execute(compiled, null as any)).toThrow();
      expect(() => predicateExecutor.execute(compiled, {} as any)).toThrow();
      expect(() => predicateExecutor.execute(compiled, { variables: null } as any)).toThrow();
    });
  });

  describe("Accessing nested variables", () => {
    it("should evaluate predicates on nested variables", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "user.roles"
      });

      const context = makeContext({
        user: { roles: [] }
      });
      expect(predicateExecutor.execute(compiled, context)).toBe(true);

      const context2 = makeContext({
        user: { roles: ["admin"] }
      });
      expect(predicateExecutor.execute(compiled, context2)).toBe(false);
    });

    it("should return true for isEmpty on non-existent nested path", () => {
      const compiled = predicateCompiler.compile({
        type: "isEmpty",
        variable: "user.profile.email"
      });

      const context = makeContext({ user: {} });
      expect(predicateExecutor.execute(compiled, context)).toBe(true);
    });
  });

  describe("Unknown predicate type", () => {
    it("should throw on unknown predicate type", () => {
      const compiled = {
        ast: {
          type: "unknownType",
          variable: "x"
        },
        dependencies: ["x"],
        complexity: 1
      };
      const context = makeContext({ x: null });

      expect(() => predicateExecutor.execute(compiled, context)).toThrow();
    });
  });
});

describe("Predicate Integration Tests", () => {
  it("should compile and execute isEmpty workflow", () => {
    const compiled = predicateCompiler.compile({
      type: "isEmpty",
      variable: "items"
    });

    const context: EvaluationContext = {
      variables: { items: [] },
      input: {},
      output: {}
    };

    const result = predicateExecutor.execute(compiled, context);
    expect(result).toBe(true);
  });

  it("should support checking output scope with proper context", () => {
    // While PredicateExecutor uses base getVariableValue which now supports all scopes
    const compiled = predicateCompiler.compile({
      type: "isEmpty",
      variable: "output.data"
    });

    const context: EvaluationContext = {
      variables: {},
      input: {},
      output: { data: [] }
    };

    const result = predicateExecutor.execute(compiled, context);
    expect(result).toBe(true);
  });
});
