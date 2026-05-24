import { describe, it, expect } from "vitest";
import { ExpressionCompiler, expressionCompiler } from "../expression-compiler.js";
import type { EvaluationContext } from "@wf-agent/types";
import { dslParse } from "../dsl/index.js";

function makeContext(variables: Record<string, unknown> = {}): EvaluationContext {
  return { variables, input: {}, output: {} };
}

describe("ExpressionCompiler", () => {
  let compiler: ExpressionCompiler;

  beforeEach(() => {
    compiler = new ExpressionCompiler();
  });

  describe("compile", () => {
    it("should compile a simple expression", () => {
      const compiled = compiler.compile("1 + 2");
      expect(compiled.ast).toBeDefined();
      expect(compiled.ast.type).toBe("binary");
    });

    it("should return a CompiledExpression with all fields", () => {
      const compiled = compiler.compile("x > 5");
      expect(typeof compiled.evaluate).toBe("function");
      expect(Array.isArray(compiled.dependencies)).toBe(true);
      expect(typeof compiled.complexity).toBe("number");
    });

    it("should evaluate compiled expression", () => {
      const compiled = compiler.compile("1 + 2");
      expect(compiled.evaluate(makeContext())).toBe(3);
    });

    it("should throw for invalid expression", () => {
      expect(() => compiler.compile("a +")).toThrow();
    });
  });

  describe("dependency extraction", () => {
    it("should extract identifier dependency", () => {
      const compiled = compiler.compile("x > 5");
      expect(compiled.dependencies).toContain("x");
    });

    it("should extract multiple dependencies", () => {
      const compiled = compiler.compile("a + b > c");
      expect(compiled.dependencies).toContain("a");
      expect(compiled.dependencies).toContain("b");
      expect(compiled.dependencies).toContain("c");
    });

    it("should extract member access as dependency", () => {
      const compiled = compiler.compile("obj.prop > 5");
      expect(compiled.dependencies).toContain("obj.prop");
    });

    it("should extract chained member access dependencies", () => {
      const compiled = compiler.compile("a.b.c > 1");
      expect(compiled.dependencies).toContain("a.b.c");
    });

    it("should not duplicate dependencies", () => {
      const compiled = compiler.compile("x + x > 5");
      const xDeps = compiled.dependencies.filter(d => d === "x");
      expect(xDeps.length).toBe(1);
    });

    it("should extract dependencies from ternary", () => {
      const compiled = compiler.compile("a ? b : c");
      expect(compiled.dependencies).toContain("a");
      expect(compiled.dependencies).toContain("b");
      expect(compiled.dependencies).toContain("c");
    });

    it("should extract dependencies from function call arguments", () => {
      const compiled = compiler.compile("myFunc(a, b)");
      expect(compiled.dependencies).toContain("a");
      expect(compiled.dependencies).toContain("b");
    });

    it("should return empty deps for literal only", () => {
      const compiled = compiler.compile("42");
      expect(compiled.dependencies).toEqual([]);
    });
  });

  describe("complexity calculation", () => {
    it("should return complexity 1 for literal", () => {
      const compiled = compiler.compile("42");
      expect(compiled.complexity).toBe(1);
    });

    it("should return complexity 1 for identifier", () => {
      const compiled = compiler.compile("x");
      expect(compiled.complexity).toBe(1);
    });

    it("should return higher complexity for binary expressions", () => {
      const simple = compiler.compile("1 + 2");
      const nested = compiler.compile("(1 + 2) * (3 + 4)");
      expect(nested.complexity).toBeGreaterThan(simple.complexity);
    });

    it("should return higher complexity for ternary", () => {
      const bin = compiler.compile("a + b");
      const tern = compiler.compile("a ? b : c");
      expect(tern.complexity).toBeGreaterThan(bin.complexity);
    });

    it("should return higher complexity for function calls", () => {
      const id = compiler.compile("x");
      const call = compiler.compile("myFunc(a)");
      expect(call.complexity).toBeGreaterThan(id.complexity);
    });
  });

  describe("caching", () => {
    it("should cache compiled expressions", () => {
      const c1 = compiler.compile("x + 1");
      const c2 = compiler.compile("x + 1");
      expect(c1).toBe(c2);
    });

    it("should differentiate expressions", () => {
      const c1 = compiler.compile("x + 1");
      const c2 = compiler.compile("x + 2");
      expect(c1).not.toBe(c2);
    });

    it("should report cache size", () => {
      compiler.compile("a");
      compiler.compile("b");
      expect(compiler.getCacheSize()).toBe(2);
    });

    it("should clear cache", () => {
      compiler.compile("a");
      compiler.compile("b");
      compiler.clearCache();
      expect(compiler.getCacheSize()).toBe(0);
    });
  });

  describe("member access path building", () => {
    it("should build path for simple member access", () => {
      const compiled = compiler.compile("obj.prop > 5");
      const deps = compiled.dependencies.filter(d => d.includes("."));
      expect(deps[0]).toBe("obj.prop");
    });

    it("should build path with array index notation", () => {
      const compiled = compiler.compile("items[0].name == 'x'");
      const deps = compiled.dependencies.filter(d => d.includes("["));
      expect(deps.some(d => d.includes("[0]"))).toBe(true);
    });

    it("should build path for deeply nested member access", () => {
      const compiled = compiler.compile("a.b.c.d > 5");
      const deps = compiled.dependencies.filter(d => d.includes("."));
      expect(deps.some(d => d.split(".").length >= 2)).toBe(true);
    });
  });
});

describe("expressionCompiler singleton", () => {
  it("should be an instance of ExpressionCompiler", () => {
    expect(expressionCompiler).toBeInstanceOf(ExpressionCompiler);
  });

  it("should compile and evaluate expressions", () => {
    const result = expressionCompiler.compile("2 * 3");
    expect(result.evaluate(makeContext())).toBe(6);
  });
});