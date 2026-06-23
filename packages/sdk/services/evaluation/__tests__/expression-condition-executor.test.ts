/**
 * ExpressionConditionExecutor Tests
 * Tests expression evaluation with all operators and methods
 */

import { describe, it, expect, beforeEach } from "vitest";
import { expressionConditionExecutor } from "../executors/expression-condition-executor.js";
import { expressionCompiler } from "../compilers/expression-compiler.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("ExpressionConditionExecutor", () => {
  const makeContext = (vars: Record<string, unknown> = {}): EvaluationContext => ({
    variables: vars,
    input: { userId: 1 },
    output: { status: "success" }
  });

  describe("Arithmetic Operators", () => {
    it("should evaluate addition", () => {
      const compiled = expressionCompiler.compile("5 + 3");
      const result = expressionConditionExecutor.execute(compiled, makeContext());
      expect(result).toBe(8);
    });

    it("should evaluate subtraction", () => {
      const compiled = expressionCompiler.compile("10 - 3");
      const result = expressionConditionExecutor.execute(compiled, makeContext());
      expect(result).toBe(7);
    });

    it("should evaluate multiplication", () => {
      const compiled = expressionCompiler.compile("4 * 5");
      const result = expressionConditionExecutor.execute(compiled, makeContext());
      expect(result).toBe(20);
    });

    it("should evaluate division", () => {
      const compiled = expressionCompiler.compile("20 / 4");
      const result = expressionConditionExecutor.execute(compiled, makeContext());
      expect(result).toBe(5);
    });

    it("should handle division by zero", () => {
      const compiled = expressionCompiler.compile("10 / 0");
      const result = expressionConditionExecutor.execute(compiled, makeContext());
      expect(isNaN(result as number)).toBe(true);
    });

    it("should evaluate modulo", () => {
      const compiled = expressionCompiler.compile("10 % 3");
      const result = expressionConditionExecutor.execute(compiled, makeContext());
      expect(result).toBe(1);
    });

    it("should respect operator precedence", () => {
      const compiled = expressionCompiler.compile("2 + 3 * 4");
      const result = expressionConditionExecutor.execute(compiled, makeContext());
      expect(result).toBe(14);
    });
  });

  describe("Comparison Operators", () => {
    it("should evaluate equality", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("5 == 5"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("5 == 3"),
        makeContext()
      )).toBe(false);
    });

    it("should evaluate inequality", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("5 != 3"),
        makeContext()
      )).toBe(true);
    });

    it("should evaluate greater than", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("5 > 3"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("3 > 5"),
        makeContext()
      )).toBe(false);
    });

    it("should evaluate less than", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("3 < 5"),
        makeContext()
      )).toBe(true);
    });

    it("should evaluate greater than or equal", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("5 >= 5"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("5 >= 3"),
        makeContext()
      )).toBe(true);
    });

    it("should evaluate less than or equal", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("3 <= 5"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("5 <= 5"),
        makeContext()
      )).toBe(true);
    });
  });

  describe("Logical Operators", () => {
    it("should evaluate AND with short-circuit", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("true && true"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("true && false"),
        makeContext()
      )).toBe(false);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("false && true"),
        makeContext()
      )).toBe(false);
    });

    it("should evaluate OR with short-circuit", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("true || false"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("false || false"),
        makeContext()
      )).toBe(false);
    });

    it("should evaluate NOT", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("!true"),
        makeContext()
      )).toBe(false);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("!false"),
        makeContext()
      )).toBe(true);
    });
  });

  describe("String Operations", () => {
    it("should evaluate contains operator", () => {
      const compiled = expressionCompiler.compile("'hello' contains 'ell'");
      expect(expressionConditionExecutor.execute(compiled, makeContext())).toBe(true);

      const compiled2 = expressionCompiler.compile("'hello' contains 'xyz'");
      expect(expressionConditionExecutor.execute(compiled2, makeContext())).toBe(false);
    });

    it("should evaluate string methods", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("'hello'.startsWith('he')"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("'hello'.endsWith('lo')"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("'HELLO'.toLowerCase() == 'hello'"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("'hello'.toUpperCase() == 'HELLO'"),
        makeContext()
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("'  hello  '.trim() == 'hello'"),
        makeContext()
      )).toBe(true);
    });
  });

  describe("Array Methods", () => {
    it("should evaluate someEqual", () => {
      const context = makeContext({
        items: [
          { id: 1, status: "active" },
          { id: 2, status: "inactive" }
        ]
      });

      const compiled = expressionCompiler.compile("items.someEqual('status', 'active')");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(true);

      const compiled2 = expressionCompiler.compile("items.someEqual('status', 'deleted')");
      expect(expressionConditionExecutor.execute(compiled2, context)).toBe(false);
    });

    it("should evaluate someContains", () => {
      const context = makeContext({
        items: [
          { name: "apple" },
          { name: "banana" }
        ]
      });

      const compiled = expressionCompiler.compile("items.someContains('name', 'app')");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(true);
    });

    it("should evaluate everyEqual", () => {
      const context = makeContext({
        statuses: [
          { type: "user" },
          { type: "user" }
        ]
      });

      const compiled = expressionCompiler.compile("statuses.everyEqual('type', 'user')");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(true);
    });

    it("should evaluate everyHas", () => {
      const context = makeContext({
        people: [
          { name: "John", role: "admin" },
          { name: "Jane", role: "user" }
        ]
      });

      const compiled = expressionCompiler.compile("people.everyHas('role')");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(true);
    });

    it("should evaluate countWhere", () => {
      const context = makeContext({
        items: [
          { status: "completed" },
          { status: "pending" },
          { status: "completed" }
        ]
      });

      const compiled = expressionCompiler.compile("items.countWhere('status', 'completed')");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(2);
    });

    it("should evaluate findEqual", () => {
      const context = makeContext({
        items: [
          { id: 1, name: "John" },
          { id: 2, name: "Jane" }
        ]
      });

      const compiled = expressionCompiler.compile("items.findEqual('id', 2)");
      const result = expressionConditionExecutor.execute(compiled, context);
      expect((result as any).name).toBe("Jane");
    });

    it("should evaluate sum on numeric array", () => {
      const context = makeContext({
        items: [
          { amount: 10 },
          { amount: 20 },
          { amount: 15 }
        ]
      });

      const compiled = expressionCompiler.compile("items.sum('amount')");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(45);
    });

    it("should evaluate avg on numeric array", () => {
      const context = makeContext({
        items: [
          { amount: 10 },
          { amount: 20 }
        ]
      });

      const compiled = expressionCompiler.compile("items.avg('amount')");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(15);
    });

    it("should evaluate min and max", () => {
      const context = makeContext({
        prices: [
          { price: 100 },
          { price: 50 },
          { price: 200 }
        ]
      });

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("prices.min('price')"),
        context
      )).toBe(50);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("prices.max('price')"),
        context
      )).toBe(200);
    });

    it("should evaluate first and last", () => {
      const context = makeContext({
        items: ["a", "b", "c"]
      });

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("items.first()"),
        context
      )).toBe("a");

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("items.last()"),
        context
      )).toBe("c");
    });

    it("should evaluate distinct", () => {
      const context = makeContext({
        items: [
          { tag: "important" },
          { tag: "bug" },
          { tag: "important" }
        ]
      });

      const compiled = expressionCompiler.compile("items.distinct('tag')");
      const result = expressionConditionExecutor.execute(compiled, context) as unknown[];
      expect(result).toHaveLength(2);
    });
  });

  describe("'in' Operator (Simplified)", () => {
    it("should check membership in array", () => {
      const context = makeContext({ roles: ["admin", "user"] });

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("'admin' in roles"),
        context
      )).toBe(true);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("'guest' in roles"),
        context
      )).toBe(false);
    });

    it("should work with literals", () => {
      const compiled = expressionCompiler.compile("1 in [1, 2, 3]");
      expect(expressionConditionExecutor.execute(compiled, makeContext())).toBe(true);

      const compiled2 = expressionCompiler.compile("5 in [1, 2, 3]");
      expect(expressionConditionExecutor.execute(compiled2, makeContext())).toBe(false);
    });

    it("should require array on right side", () => {
      const context = makeContext({ status: "active" });

      const compiled = expressionCompiler.compile("1 in status");
      expect(expressionConditionExecutor.execute(compiled, context)).toBe(false);
    });
  });

  describe("Ternary Operator", () => {
    it("should evaluate ternary expressions", () => {
      const compiled = expressionCompiler.compile("x > 5 ? 'big' : 'small'");

      expect(expressionConditionExecutor.execute(
        compiled,
        makeContext({ x: 10 })
      )).toBe("big");

      expect(expressionConditionExecutor.execute(
        compiled,
        makeContext({ x: 3 })
      )).toBe("small");
    });
  });

  describe("Member Access and Literals", () => {
    it("should access nested object properties", () => {
      const context = makeContext({
        user: {
          profile: {
            name: "John",
            email: "john@example.com"
          }
        }
      });

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("user.profile.name == 'John'"),
        context
      )).toBe(true);
    });

    it("should access array elements", () => {
      const context = makeContext({
        items: ["a", "b", "c"]
      });

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("items[0] == 'a'"),
        context
      )).toBe(true);
    });

    it("should handle undefined properties", () => {
      const context = makeContext({
        user: { name: "John" }
      });

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("user.email == undefined"),
        context
      )).toBe(true);
    });
  });

  describe("Unary Minus", () => {
    it("should negate numbers", () => {
      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("-5"),
        makeContext()
      )).toBe(-5);

      expect(expressionConditionExecutor.execute(
        expressionCompiler.compile("-(3 + 2)"),
        makeContext()
      )).toBe(-5);
    });

    it("should return NaN for non-numeric", () => {
      const result = expressionConditionExecutor.execute(
        expressionCompiler.compile("-'hello'"),
        makeContext()
      );
      expect(isNaN(result as number)).toBe(true);
    });
  });

  describe("Array Literals", () => {
    it("should create array literals", () => {
      const result = expressionConditionExecutor.execute(
        expressionCompiler.compile("[1, 2, 3]"),
        makeContext()
      );
      expect(result).toEqual([1, 2, 3]);
    });
  });

  describe("Security", () => {
    it("should block access to __proto__", () => {
      expect(() => expressionConditionExecutor.execute(
        expressionCompiler.compile("x.__proto__"),
        makeContext({ x: {} })
      )).toThrow();
    });

    it("should block access to constructor", () => {
      expect(() => expressionConditionExecutor.execute(
        expressionCompiler.compile("x.constructor"),
        makeContext({ x: {} })
      )).toThrow();
    });

    it("should block access to prototype", () => {
      expect(() => expressionConditionExecutor.execute(
        expressionCompiler.compile("x.prototype"),
        makeContext({ x: {} })
      )).toThrow();
    });
  });

  describe("Custom Function Registration", () => {
    it("should register and call custom functions", () => {
      expressionConditionExecutor.registerFunction("double", (x: unknown) => {
        return (x as number) * 2;
      });

      const result = expressionConditionExecutor.execute(
        expressionCompiler.compile("double(5)"),
        makeContext()
      );
      expect(result).toBe(10);

      expressionConditionExecutor.unregisterFunction("double");
    });

    it("should throw on calling unregistered functions", () => {
      expect(() => expressionConditionExecutor.execute(
        expressionCompiler.compile("unknownFunc(5)"),
        makeContext()
      )).toThrow();
    });

    it("should list registered functions", () => {
      expressionConditionExecutor.registerFunction("test1", () => 1);
      expressionConditionExecutor.registerFunction("test2", () => 2);

      const funcs = expressionConditionExecutor.getRegisteredFunctions();
      expect(funcs).toContain("test1");
      expect(funcs).toContain("test2");

      expressionConditionExecutor.unregisterFunction("test1");
      expressionConditionExecutor.unregisterFunction("test2");
    });
  });
});
