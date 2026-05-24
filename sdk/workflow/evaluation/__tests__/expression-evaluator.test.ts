import { describe, it, expect, beforeEach } from "vitest";
import { ExpressionEvaluator } from "../expression-evaluator.js";
import { ExpressionSecurityError, RuntimeValidationError } from "@wf-agent/types";
import type { EvaluationContext } from "@wf-agent/types";
import type { Expression } from "../dsl/types.js";
import { dslParse } from "../dsl/index.js";

function makeContext(variables: Record<string, unknown> = {}): EvaluationContext {
  return { variables, input: {}, output: {} };
}

describe("ExpressionEvaluator", () => {
  let evaluator: ExpressionEvaluator;

  beforeEach(() => {
    evaluator = new ExpressionEvaluator();
  });

  describe("evaluate (string expression)", () => {
    it("should evaluate literal expressions", () => {
      expect(evaluator.evaluate("42", makeContext())).toBe(42);
      expect(evaluator.evaluate("'hello'", makeContext())).toBe("hello");
      expect(evaluator.evaluate("true", makeContext())).toBe(true);
      expect(evaluator.evaluate("false", makeContext())).toBe(false);
      expect(evaluator.evaluate("null", makeContext())).toBeNull();
    });

    it("should evaluate arithmetic expressions", () => {
      expect(evaluator.evaluate("1 + 2", makeContext())).toBe(3);
      expect(evaluator.evaluate("10 - 4", makeContext())).toBe(6);
      expect(evaluator.evaluate("3 * 4", makeContext())).toBe(12);
      expect(evaluator.evaluate("10 / 2", makeContext())).toBe(5);
      expect(evaluator.evaluate("10 % 3", makeContext())).toBe(1);
    });

    it("should respect arithmetic precedence", () => {
      expect(evaluator.evaluate("2 + 3 * 4", makeContext())).toBe(14);
      expect(evaluator.evaluate("(2 + 3) * 4", makeContext())).toBe(20);
    });

    it("should evaluate comparison expressions", () => {
      expect(evaluator.evaluate("5 > 3", makeContext())).toBe(true);
      expect(evaluator.evaluate("5 < 3", makeContext())).toBe(false);
      expect(evaluator.evaluate("3 >= 3", makeContext())).toBe(true);
      expect(evaluator.evaluate("3 <= 3", makeContext())).toBe(true);
      expect(evaluator.evaluate("3 == 3", makeContext())).toBe(true);
      expect(evaluator.evaluate("3 != 3", makeContext())).toBe(false);
    });

    it("should evaluate logical expressions", () => {
      expect(evaluator.evaluate("true && false", makeContext())).toBe(false);
      expect(evaluator.evaluate("true || false", makeContext())).toBe(true);
      expect(evaluator.evaluate("!false", makeContext())).toBe(true);
    });

    it("should evaluate variable references", () => {
      const ctx = makeContext({ x: 10, y: 5 });
      expect(evaluator.evaluate("x + y", ctx)).toBe(15);
    });

    it("should evaluate member access expressions", () => {
      const ctx = makeContext({ user: { name: "Alice", age: 30 } });
      expect(evaluator.evaluate("user.name", ctx)).toBe("Alice");
      expect(evaluator.evaluate("user.age", ctx)).toBe(30);
    });

    it("should evaluate array indexing", () => {
      const ctx = makeContext({ items: [{ id: 1 }, { id: 2 }] });
      expect(evaluator.evaluate("items[0].id", ctx)).toBe(1);
      expect(evaluator.evaluate("items[1].id", ctx)).toBe(2);
    });

    it("should evaluate ternary expressions", () => {
      const ctx = makeContext({ a: true, b: 1, c: 2 });
      expect(evaluator.evaluate("a ? b : c", ctx)).toBe(1);

      const ctx2 = makeContext({ a: false, b: 1, c: 2 });
      expect(evaluator.evaluate("a ? b : c", ctx2)).toBe(2);
    });

    it("should evaluate contains expression", () => {
      expect(evaluator.evaluate("'hello' contains 'ell'", makeContext())).toBe(true);
      expect(evaluator.evaluate("'hello' contains 'world'", makeContext())).toBe(false);
    });

    it("should evaluate 'in' expression with array", () => {
      expect(evaluator.evaluate("5 in [1, 2, 3, 5]", makeContext())).toBe(true);
      expect(evaluator.evaluate("5 in [1, 2, 3]", makeContext())).toBe(false);
    });

    it("should evaluate unary minus", () => {
      expect(evaluator.evaluate("--5", makeContext())).toBe(5);
      expect(evaluator.evaluate("-5", makeContext())).toBe(-5);
    });

    it("should evaluate array literals", () => {
      const result = evaluator.evaluate("[1, 2, 3]", makeContext()) as unknown[];
      expect(result).toEqual([1, 2, 3]);
    });

    it("should throw for null/undefined input expression", () => {
      expect(() => evaluator.evaluate("", makeContext())).toThrow(ExpressionSecurityError);
    });

    it("should return false for undefined variable comparison", () => {
      const ctx = makeContext({});
      expect(evaluator.evaluate("undefinedVar > 5", ctx)).toBe(false);
    });
  });

  describe("evaluateAST", () => {
    it("should evaluate literal AST node", () => {
      const ast = dslParse("42");
      expect(evaluator.evaluateAST(ast, makeContext())).toBe(42);
    });

    it("should evaluate identifier AST node", () => {
      const ast = dslParse("x");
      expect(evaluator.evaluateAST(ast, makeContext({ x: 42 }))).toBe(42);
    });

    it("should evaluate binary AST node", () => {
      const ast = dslParse("1 + 2");
      expect(evaluator.evaluateAST(ast, makeContext())).toBe(3);
    });

    it("should evaluate call AST node", () => {
      evaluator.registerFunction("double", (x: number) => x * 2);
      const ast = dslParse("double(5)");
      expect(evaluator.evaluateAST(ast, makeContext())).toBe(10);
    });

    it("should throw for unknown node type", () => {
      const fakeNode = { type: "unknown" } as unknown as Expression;
      expect(() => evaluator.evaluateAST(fakeNode, makeContext())).toThrow(RuntimeValidationError);
    });
  });

  describe("string methods", () => {
    it("should evaluate startsWith", () => {
      const ctx = makeContext({ str: "Hello World" });
      expect(evaluator.evaluate("str.startsWith('Hello')", ctx)).toBe(true);
    });

    it("should evaluate endsWith", () => {
      const ctx = makeContext({ str: "Hello World" });
      expect(evaluator.evaluate("str.endsWith('World')", ctx)).toBe(true);
    });

    it("should evaluate toLowerCase", () => {
      const ctx = makeContext({ str: "HELLO" });
      expect(evaluator.evaluate("str.toLowerCase()", ctx)).toBe("hello");
    });

    it("should evaluate toUpperCase", () => {
      const ctx = makeContext({ str: "hello" });
      expect(evaluator.evaluate("str.toUpperCase()", ctx)).toBe("HELLO");
    });

    it("should evaluate trim", () => {
      const ctx = makeContext({ str: "  hello  " });
      expect(evaluator.evaluate("str.trim()", ctx)).toBe("hello");
    });
  });

  describe("array methods", () => {
    const ctx = makeContext({
      items: [
        { id: 1, name: "a", score: 80 },
        { id: 2, name: "b", score: 90 },
        { id: 3, name: "a", score: 70 },
      ],
    });

    it("should evaluate someEqual", () => {
      expect(evaluator.evaluate("items.someEqual('id', 2)", ctx)).toBe(true);
      expect(evaluator.evaluate("items.someEqual('id', 99)", ctx)).toBe(false);
    });

    it("should evaluate everyEqual", () => {
      expect(evaluator.evaluate("items.everyEqual('name', 'a')", ctx)).toBe(false);
    });

    it("should evaluate countWhere", () => {
      expect(evaluator.evaluate("items.countWhere('name', 'a')", ctx)).toBe(2);
    });

    it("should evaluate findEqual", () => {
      const result = evaluator.evaluate("items.findEqual('id', 2)", ctx) as any;
      expect(result?.id).toBe(2);
    });

    it("should evaluate has", () => {
      expect(evaluator.evaluate("items.has('id', 1)", ctx)).toBe(true);
    });

    it("should evaluate sum", () => {
      expect(evaluator.evaluate("items.sum('score')", ctx)).toBe(240);
    });

    it("should evaluate avg", () => {
      expect(evaluator.evaluate("items.avg('score')", ctx)).toBe(80);
    });

    it("should evaluate min and max", () => {
      expect(evaluator.evaluate("items.min('score')", ctx)).toBe(70);
      expect(evaluator.evaluate("items.max('score')", ctx)).toBe(90);
    });

    it("should evaluate first and last", () => {
      const first = evaluator.evaluate("items.first()", ctx) as any;
      expect(first?.id).toBe(1);
      const last = evaluator.evaluate("items.last()", ctx) as any;
      expect(last?.id).toBe(3);
    });

    it("should evaluate map", () => {
      const result = evaluator.evaluate("items.map('id')", ctx) as number[];
      expect(result).toEqual([1, 2, 3]);
    });

    it("should evaluate distinct", () => {
      const result = evaluator.evaluate("items.distinct('name')", ctx) as string[];
      expect(result).toContain("a");
      expect(result).toContain("b");
    });

    it("should evaluate someContains", () => {
      const ctx2 = makeContext({
        items: [{ id: 1, tag: "hello" }, { id: 2, tag: "world" }],
      });
      expect(evaluator.evaluate("items.someContains('tag', 'ello')", ctx2)).toBe(true);
      expect(evaluator.evaluate("items.someContains('tag', 'xyz')", ctx2)).toBe(false);
    });

    it("should evaluate everyHas", () => {
      expect(evaluator.evaluate("items.everyHas('id', '')", ctx)).toBe(true);
    });

    it("should evaluate someGreaterThan", () => {
      expect(evaluator.evaluate("items.someGreaterThan('score', 85)", ctx)).toBe(true);
      expect(evaluator.evaluate("items.someGreaterThan('score', 100)", ctx)).toBe(false);
    });

    it("should evaluate someLessThan", () => {
      expect(evaluator.evaluate("items.someLessThan('score', 75)", ctx)).toBe(true);
    });

    it("should evaluate everyGreaterThan and everyLessThan", () => {
      expect(evaluator.evaluate("items.everyGreaterThan('score', 50)", ctx)).toBe(true);
      expect(evaluator.evaluate("items.everyLessThan('score', 100)", ctx)).toBe(true);
    });

    it("should handle empty array methods", () => {
      const emptyCtx = makeContext({ empty: [] });
      expect(evaluator.evaluate("empty.someEqual(id, 1)", emptyCtx)).toBe(false);
      expect(evaluator.evaluate("empty.everyEqual(id, 1)", emptyCtx)).toBe(true);
      expect(evaluator.evaluate("empty.first()", emptyCtx)).toBeNull();
    });

    it("should handle null/undefined array", () => {
      const nullCtx = makeContext({ n: null, u: undefined });
      expect(evaluator.evaluate("n.someEqual(id, 1)", nullCtx)).toBe(false);
      expect(evaluator.evaluate("u.someEqual(id, 1)", nullCtx)).toBe(false);
    });
  });

  describe("custom functions", () => {
    it("should register and call custom functions", () => {
      evaluator.registerFunction("add", (a: number, b: number) => a + b);
      evaluator.registerFunction("multiply", (a: number, b: number) => a * b);

      const ctx = makeContext({});
      expect(evaluator.evaluate("add(1, 2)", ctx)).toBe(3);
      expect(evaluator.evaluate("multiply(3, 4)", ctx)).toBe(12);
    });

    it("should unregister functions", () => {
      evaluator.registerFunction("tempFn", () => 42);
      evaluator.unregisterFunction("tempFn");
      const ctx = makeContext({});
      expect(() => evaluator.evaluate("tempFn()", ctx)).toThrow(RuntimeValidationError);
    });

    it("should list registered functions", () => {
      evaluator.registerFunction("fn1", () => 1);
      evaluator.registerFunction("fn2", () => 2);
      const functions = evaluator.getRegisteredFunctions();
      expect(functions).toContain("fn1");
      expect(functions).toContain("fn2");
    });

    it("should throw for unknown function", () => {
      const ctx = makeContext({});
      expect(() => evaluator.evaluate("unknownFn()", ctx)).toThrow(RuntimeValidationError);
    });

    it("should propagate function errors", () => {
      evaluator.registerFunction("errorFn", () => {
        throw new Error("Function error");
      });
      const ctx = makeContext({});
      expect(() => evaluator.evaluate("errorFn()", ctx)).toThrow(RuntimeValidationError);
    });
  });

  describe("forbidden property access", () => {
    it("should block __proto__ access", () => {
      const ctx = makeContext({ obj: {} });
      expect(() => evaluator.evaluate("obj.__proto__", ctx)).toThrow(ExpressionSecurityError);
    });

    it("should block constructor access", () => {
      const ctx = makeContext({ obj: {} });
      expect(() => evaluator.evaluate("obj.constructor", ctx)).toThrow(ExpressionSecurityError);
    });

    it("should block prototype access", () => {
      const ctx = makeContext({ obj: {} });
      expect(() => evaluator.evaluate("obj.prototype", ctx)).toThrow(ExpressionSecurityError);
    });
  });

  describe("division by zero", () => {
    it("should return NaN for division by zero", () => {
      const result = evaluator.evaluate("5 / 0", makeContext());
      expect(Number.isNaN(result)).toBe(true);
    });

    it("should return NaN for modulo zero", () => {
      const result = evaluator.evaluate("5 % 0", makeContext());
      expect(Number.isNaN(result)).toBe(true);
    });
  });

  describe("short-circuit evaluation", () => {
    it("should short-circuit AND", () => {
      const ctx = makeContext({ x: false });
      expect(evaluator.evaluate("x && missingVar", ctx)).toBe(false);
    });

    it("should short-circuit OR", () => {
      const ctx = makeContext({ x: true });
      expect(evaluator.evaluate("x || missingVar", ctx)).toBe(true);
    });
  });

  describe("context variable access", () => {
    it("should resolve simple variables from context.variables", () => {
      const ctx = makeContext({ x: 10 });
      expect(evaluator.evaluate("x * 2", ctx)).toBe(20);
    });

    it("should resolve input variables", () => {
      const ctx = makeContext({});
      (ctx as any).input = { data: 42 };
      expect(evaluator.evaluate("input.data", ctx)).toBe(42);
    });

    it("should resolve output variables", () => {
      const ctx = makeContext({});
      (ctx as any).output = { result: "ok" };
      expect(evaluator.evaluate("output.result", ctx)).toBe("ok");
    });
  });
});