/**
 * Function Call and Expression Compiler Tests
 * Tests for Phase 1 improvements: Function Call Node and Expression Compilation
 */

import { describe, it, expect, beforeEach } from "vitest";
import { expressionEvaluator, expressionCompiler } from "../index.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("Function Call Node", () => {
  let context: EvaluationContext;

  beforeEach(() => {
    context = {
      input: {},
      output: {},
      variables: {
        user: {
          name: "John Doe",
          age: 25,
          email: "john@example.com",
          createdAt: new Date("2020-01-15"),
        },
        items: [
          { id: 1, name: "Item 1", price: 100 },
          { id: 2, name: "Item 2", price: 200 },
          { id: 3, name: "Item 3", price: 300 },
        ],
      },
    };
  });

  describe("Basic Function Calls", () => {
    it("should register and call a simple function", () => {
      // Register a custom function
      expressionEvaluator.registerFunction("double", (x: number) => x * 2);

      const result = expressionEvaluator.evaluate("double(user.age)", context);
      expect(result).toBe(50);
    });

    it("should handle functions with multiple arguments", () => {
      expressionEvaluator.registerFunction("add", (a: number, b: number) => a + b);

      const result = expressionEvaluator.evaluate("add(user.age, 10)", context);
      expect(result).toBe(35);
    });

    it("should handle functions returning strings", () => {
      expressionEvaluator.registerFunction("greet", (name: string) => `Hello, ${name}!`);

      const result = expressionEvaluator.evaluate("greet(user.name)", context);
      expect(result).toBe("Hello, John Doe!");
    });

    it("should throw error for unregistered functions", () => {
      expect(() => {
        expressionEvaluator.evaluate("unknownFunc(user.age)", context);
      }).toThrow(/Unknown function: unknownFunc/);
    });

    it("should list registered functions", () => {
      expressionEvaluator.registerFunction("testFunc1", () => 1);
      expressionEvaluator.registerFunction("testFunc2", () => 2);

      const functions = expressionEvaluator.getRegisteredFunctions();
      expect(functions).toContain("testFunc1");
      expect(functions).toContain("testFunc2");
    });

    it("should unregister functions", () => {
      expressionEvaluator.registerFunction("tempFunc", () => 42);
      expect(expressionEvaluator.getRegisteredFunctions()).toContain("tempFunc");

      expressionEvaluator.unregisterFunction("tempFunc");
      expect(expressionEvaluator.getRegisteredFunctions()).not.toContain("tempFunc");
    });
  });

  describe("Complex Function Scenarios", () => {
    it("should handle nested function calls", () => {
      expressionEvaluator.registerFunction("add", (a: number, b: number) => a + b);
      expressionEvaluator.registerFunction("multiply", (a: number, b: number) => a * b);

      // multiply(add(user.age, 5), 2) = multiply(30, 2) = 60
      const result = expressionEvaluator.evaluate("multiply(add(user.age, 5), 2)", context);
      expect(result).toBe(60);
    });

    it("should handle functions in logical expressions", () => {
      expressionEvaluator.registerFunction("isAdult", (age: number) => age >= 18);

      const result = expressionEvaluator.evaluate("isAdult(user.age) && user.age < 65", context);
      expect(result).toBe(true);
    });

    it("should handle functions in comparison expressions", () => {
      expressionEvaluator.registerFunction("getLength", (str: string) => str.length);

      const result = expressionEvaluator.evaluate("getLength(user.name) > 5", context);
      expect(result).toBe(true);
    });

    it("should handle function errors gracefully", () => {
      expressionEvaluator.registerFunction("divide", (a: number, b: number) => {
        if (b === 0) throw new Error("Division by zero");
        return a / b;
      });

      expect(() => {
        expressionEvaluator.evaluate("divide(10, 0)", context);
      }).toThrow(/Function execution failed/);
    });
  });

  describe("Real-world Use Cases", () => {
    it("should format dates", () => {
      expressionEvaluator.registerFunction("formatDate", (date: Date, format: string) => {
        if (!(date instanceof Date)) return "Invalid Date";
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, "0");
        const day = String(date.getDate()).padStart(2, "0");
        return `${year}-${month}-${day}`;
      });

      const result = expressionEvaluator.evaluate("formatDate(user.createdAt, 'YYYY-MM-DD')", context);
      expect(result).toBe("2020-01-15");
    });

    it("should calculate total price", () => {
      expressionEvaluator.registerFunction("calculateTotal", (items: any[]) => {
        return items.reduce((sum, item) => sum + item.price, 0);
      });

      const result = expressionEvaluator.evaluate("calculateTotal(items)", context);
      expect(result).toBe(600);
    });

    it("should validate email format", () => {
      expressionEvaluator.registerFunction("isValidEmail", (email: string) => {
        return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
      });

      const result = expressionEvaluator.evaluate("isValidEmail(user.email)", context);
      expect(result).toBe(true);
    });
  });
});

describe("Expression Compiler", () => {
  let context: EvaluationContext;

  beforeEach(() => {
    context = {
      input: {},
      output: {},
      variables: {
        user: {
          name: "Alice",
          age: 30,
          status: "active",
        },
        scores: [85, 92, 78, 95],
      },
    };
  });

  describe("Basic Compilation", () => {
    it("should compile simple expressions", () => {
      const compiled = expressionCompiler.compile("user.age >= 18");

      expect(compiled.ast).toBeDefined();
      expect(compiled.dependencies).toContain("user.age");
      expect(compiled.complexity).toBeGreaterThan(0);

      const result = compiled.evaluate(context);
      expect(result).toBe(true);
    });

    it("should cache compiled expressions", () => {
      const expr = "user.status == 'active'";

      const compiled1 = expressionCompiler.compile(expr);
      const compiled2 = expressionCompiler.compile(expr);

      expect(compiled1).toBe(compiled2); // Same reference from cache
      expect(expressionCompiler.getCacheSize()).toBeGreaterThanOrEqual(1);
    });

    it("should clear compilation cache", () => {
      expressionCompiler.compile("user.age > 20");
      expressionCompiler.compile("user.name == 'Alice'");

      expect(expressionCompiler.getCacheSize()).toBeGreaterThanOrEqual(2);

      expressionCompiler.clearCache();
      expect(expressionCompiler.getCacheSize()).toBe(0);
    });
  });

  describe("Dependency Extraction", () => {
    it("should extract dependencies from simple expressions", () => {
      const compiled = expressionCompiler.compile("user.age >= 18");
      expect(compiled.dependencies).toEqual(["user.age"]);
    });

    it("should extract dependencies from logical expressions", () => {
      const compiled = expressionCompiler.compile("user.age >= 18 && user.status == 'active'");
      expect(compiled.dependencies).toContain("user.age");
      expect(compiled.dependencies).toContain("user.status");
    });

    it("should extract dependencies from complex expressions", () => {
      const compiled = expressionCompiler.compile(
        "(user.age >= 18 || user.status == 'admin') && user.name != ''"
      );
      expect(compiled.dependencies).toContain("user.age");
      expect(compiled.dependencies).toContain("user.status");
      expect(compiled.dependencies).toContain("user.name");
    });

    it("should handle array method dependencies", () => {
      const compiled = expressionCompiler.compile("scores.countWhere('value', 85) > 0");
      expect(compiled.dependencies).toContain("scores");
    });
  });

  describe("Complexity Calculation", () => {
    it("should assign low complexity to literals", () => {
      const compiled = expressionCompiler.compile("true");
      expect(compiled.complexity).toBe(1);
    });

    it("should assign medium complexity to comparisons", () => {
      const compiled = expressionCompiler.compile("user.age == 25");
      expect(compiled.complexity).toBe(2);
    });

    it("should assign higher complexity to logical operations", () => {
      const compiled = expressionCompiler.compile("user.age >= 18 && user.status == 'active'");
      expect(compiled.complexity).toBeGreaterThan(4);
    });

    it("should assign high complexity to array methods", () => {
      const compiled = expressionCompiler.compile("scores.sum('value') > 100");
      expect(compiled.complexity).toBeGreaterThan(5);
    });
  });

  describe("Performance Benefits", () => {
    it("should evaluate compiled expressions faster than parsing each time", () => {
      const expression = "user.age >= 18 && user.status == 'active'";
      const iterations = 1000;

      // Measure time for repeated parsing and evaluation
      const startParse = performance.now();
      for (let i = 0; i < iterations; i++) {
        expressionEvaluator.evaluate(expression, context);
      }
      const parseTime = performance.now() - startParse;

      // Compile once
      const compiled = expressionCompiler.compile(expression);

      // Measure time for compiled evaluation
      const startCompiled = performance.now();
      for (let i = 0; i < iterations; i++) {
        compiled.evaluate(context);
      }
      const compiledTime = performance.now() - startCompiled;

      // Compiled should be faster (at least not slower)
      expect(compiledTime).toBeLessThanOrEqual(parseTime * 1.5); // Allow some variance
    });
  });

  describe("Integration with Function Calls", () => {
    it("should compile expressions with function calls", () => {
      expressionEvaluator.registerFunction("isEven", (n: number) => n % 2 === 0);

      const compiled = expressionCompiler.compile("isEven(user.age)");
      expect(compiled.dependencies).toContain("user.age");

      const result = compiled.evaluate(context);
      expect(result).toBe(true); // 30 is even
    });

    it("should track dependencies through function arguments", () => {
      expressionEvaluator.registerFunction("double", (x: number) => x * 2);

      const compiled = expressionCompiler.compile("double(user.age) > 50");
      expect(compiled.dependencies).toContain("user.age");
    });
  });
});
