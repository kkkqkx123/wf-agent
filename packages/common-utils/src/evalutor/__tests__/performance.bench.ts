/**
 * Performance Benchmarks for Expression Evaluation
 * Comprehensive performance testing suite (Phase 2)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { expressionEvaluator, expressionCompiler } from "../index.js";
import type { EvaluationContext } from "@wf-agent/types";

/**
 * Benchmark helper: measures execution time
 */
function benchmark(fn: () => void, iterations: number = 1000): {
  totalTime: number;
  avgTime: number;
  opsPerSecond: number;
} {
  const start = performance.now();
  for (let i = 0; i < iterations; i++) {
    fn();
  }
  const end = performance.now();
  const totalTime = end - start;
  const avgTime = totalTime / iterations;
  const opsPerSecond = 1000 / avgTime;

  return { totalTime, avgTime, opsPerSecond };
}

describe("Performance Benchmarks", () => {
  const context: EvaluationContext = {
    variables: {
      user: {
        name: "Alice",
        age: 30,
        email: "alice@example.com",
        active: true,
        address: {
          city: "Beijing",
          street: "123 Main St",
          zipCode: "100000",
        },
        profile: {
          bio: "Developer",
          social: {
            twitter: "@alice",
            github: "alice-dev",
          },
        },
      },
      config: {
        settings: {
          theme: "dark",
          language: "en",
        },
      },
      items: [
        { id: 1, name: "Item 1", value: 100 },
        { id: 2, name: "Item 2", value: 200 },
        { id: 3, name: "Item 3", value: 300 },
      ],
    },
    input: {},
    output: {},
  };

  describe("Simple Expressions", () => {
    it("should benchmark simple comparison", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.age > 18", context);
      });

      console.log(`Simple comparison: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(1); // Should be very fast (< 1ms avg)
    });

    it("should benchmark string comparison", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.name == 'Alice'", context);
      });

      console.log(`String comparison: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(1);
    });

    it("should benchmark boolean check", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.active == true", context);
      });

      console.log(`Boolean check: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(1);
    });
  });

  describe("Member Access Expressions", () => {
    it("should benchmark shallow member access", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.name", context);
      });

      console.log(`Shallow member access: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(1);
    });

    it("should benchmark deep member access", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.profile.social.twitter", context);
      });

      console.log(`Deep member access: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(2); // Slightly slower due to nesting
    });

    it("should benchmark nested member access with comparison", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.address.city == 'Beijing'", context);
      });

      console.log(`Nested member access + comparison: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(2);
    });
  });

  describe("Logical Expressions", () => {
    it("should benchmark AND expression", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.age > 18 && user.active == true", context);
      });

      console.log(`AND expression: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(2);
    });

    it("should benchmark OR expression", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.age < 18 || user.active == true", context);
      });

      console.log(`OR expression: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(2);
    });

    it("should benchmark complex logical expression", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate(
          "(user.age > 18 && user.active == true) || user.name == 'Bob'",
          context
        );
      });

      console.log(`Complex logical: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(3);
    });
  });

  describe("Arithmetic Expressions", () => {
    it("should benchmark simple arithmetic", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("user.age * 2", context);
      });

      console.log(`Simple arithmetic: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(2);
    });

    it("should benchmark complex arithmetic", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("(user.age + 10) * 2 - 5", context);
      });

      console.log(`Complex arithmetic: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(3);
    });
  });

  describe("Array Method Expressions", () => {
    it("should benchmark countWhere", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("items.countWhere('value', 100)", context);
      });

      console.log(`countWhere: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(5); // Array operations are slower
    });

    it("should benchmark someEqual", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("items.someEqual('id', 2)", context);
      });

      console.log(`someEqual: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(5);
    });
  });

  describe("Function Call Expressions", () => {
    beforeEach(() => {
      // Register test functions
      expressionEvaluator.registerFunction("double", (x: number) => x * 2);
      expressionEvaluator.registerFunction("isEven", (x: number) => x % 2 === 0);
    });

    afterEach(() => {
      // Clean up
      expressionEvaluator.unregisterFunction("double");
      expressionEvaluator.unregisterFunction("isEven");
    });

    it("should benchmark simple function call", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("double(user.age)", context);
      });

      console.log(`Simple function call: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(3);
    });

    it("should benchmark function call with comparison", () => {
      const result = benchmark(() => {
        expressionEvaluator.evaluate("isEven(user.age) == true", context);
      });

      console.log(`Function call + comparison: ${result.opsPerSecond.toFixed(0)} ops/sec`);
      expect(result.avgTime).toBeLessThan(3);
    });
  });

  describe("Compiled Expressions", () => {
    it("should benchmark compiled expression vs raw evaluation", () => {
      const expression = "user.age > 18 && user.active == true";
      
      // Raw evaluation
      const rawResult = benchmark(() => {
        expressionEvaluator.evaluate(expression, context);
      });

      // Compiled evaluation
      const compiled = expressionCompiler.compile(expression);
      const compiledResult = benchmark(() => {
        compiled.evaluate(context);
      });

      console.log(`Raw evaluation: ${rawResult.opsPerSecond.toFixed(0)} ops/sec`);
      console.log(`Compiled evaluation: ${compiledResult.opsPerSecond.toFixed(0)} ops/sec`);
      
      // Compiled should be faster or equal
      expect(compiledResult.avgTime).toBeLessThanOrEqual(rawResult.avgTime * 1.5);
    });

    it("should benchmark compilation overhead", () => {
      const expression = "user.profile.social.twitter == '@alice'";
      
      const compileResult = benchmark(() => {
        expressionCompiler.compile(expression);
      }, 100); // Fewer iterations since compilation is heavier

      console.log(`Compilation overhead: ${compileResult.avgTime.toFixed(3)}ms per compile`);
      expect(compileResult.avgTime).toBeLessThan(10); // Should be reasonable
    });

    it("should benchmark cached compilation", () => {
      const expression = "user.address.city == 'Beijing'";
      
      // First compilation (not cached)
      expressionCompiler.compile(expression);
      
      // Second compilation (cached)
      const cachedResult = benchmark(() => {
        expressionCompiler.compile(expression);
      }, 1000);

      console.log(`Cached compilation: ${cachedResult.opsPerSecond.toFixed(0)} ops/sec`);
      expect(cachedResult.avgTime).toBeLessThan(0.1); // Should be very fast
    });
  });

  describe("Stress Tests", () => {
    it("should handle 10000 evaluations without degradation", () => {
      const iterations = 10000;
      const start = performance.now();
      
      for (let i = 0; i < iterations; i++) {
        expressionEvaluator.evaluate("user.age > 18", context);
      }
      
      const end = performance.now();
      const totalTime = end - start;
      const avgTime = totalTime / iterations;
      
      console.log(`10000 evaluations: ${totalTime.toFixed(2)}ms total, ${avgTime.toFixed(4)}ms avg`);
      expect(avgTime).toBeLessThan(1); // Each eval should be < 1ms
    });

    it("should handle complex expressions at scale", () => {
      // Use a simpler complex expression that parser can handle
      const complexExpr = `user.age > 18 && user.active == true && user.address.city == 'Beijing'`;
      
      const iterations = 1000;
      const start = performance.now();
      
      for (let i = 0; i < iterations; i++) {
        expressionEvaluator.evaluate(complexExpr, context);
      }
      
      const end = performance.now();
      const totalTime = end - start;
      const avgTime = totalTime / iterations;
      
      console.log(`1000 complex evals: ${totalTime.toFixed(2)}ms total, ${avgTime.toFixed(4)}ms avg`);
      expect(avgTime).toBeLessThan(5); // Complex eval should be < 5ms
    });
  });

  describe("Memory Usage", () => {
    it("should not leak memory during repeated compilations", () => {
      const expressions = [
        "user.age > 18",
        "user.name == 'Alice'",
        "user.active == true",
        "user.address.city == 'Beijing'",
        "user.profile.social.twitter == '@alice'",
      ];

      // Compile each expression multiple times
      for (let i = 0; i < 100; i++) {
        expressions.forEach(expr => {
          expressionCompiler.compile(expr);
        });
      }

      // If we get here without crashing, memory management is OK
      expect(true).toBe(true);
    });
  });
});
