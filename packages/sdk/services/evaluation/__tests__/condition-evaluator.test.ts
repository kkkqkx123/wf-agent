/**
 * ConditionEvaluator Tests
 * Tests unified condition evaluation with caching and error handling
 */

import { describe, it, expect, beforeEach } from "vitest";
import { conditionEvaluator } from "../condition-evaluator.js";
import { cacheManager } from "../cache-manager.js";
import type { EvaluationContext } from "@wf-agent/types";
import { ExpressionSecurityError } from "@wf-agent/types";

describe("ConditionEvaluator", () => {
  const makeContext = (vars: Record<string, unknown> = {}): EvaluationContext => ({
    variables: vars,
    input: { userId: 1, roles: ["user"] },
    output: { status: "success", data: { count: 10 } }
  });

  beforeEach(() => {
    cacheManager.clear();
  });

  describe("Expression Conditions", () => {
    it("should evaluate simple expression", () => {
      const condition = {
        type: "expression",
        expression: "x > 5"
      };
      const context = makeContext({ x: 10 });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should evaluate expression as false", () => {
      const condition = {
        type: "expression",
        expression: "x > 5"
      };
      const context = makeContext({ x: 3 });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(false);
    });

    it("should evaluate complex expression with multiple variables", () => {
      const condition = {
        type: "expression",
        expression: "x > 5 && y < 20"
      };
      const context = makeContext({ x: 10, y: 15 });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should access input scope in expression", () => {
      const condition = {
        type: "expression",
        expression: "input.userId > 0"
      };
      const context = makeContext();

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should access output scope in expression", () => {
      const condition = {
        type: "expression",
        expression: "output.status == 'success'"
      };
      const context = makeContext();

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });
  });

  describe("Predicate Conditions", () => {
    it("should evaluate isEmpty predicate", () => {
      const condition = {
        type: "predicate",
        predicateType: "isEmpty",
        variable: "items"
      };
      const context = makeContext({ items: [] });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should evaluate isNotEmpty predicate", () => {
      const condition = {
        type: "predicate",
        predicateType: "isNotEmpty",
        variable: "items"
      };
      const context = makeContext({ items: [1, 2, 3] });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should evaluate isNull predicate", () => {
      const condition = {
        type: "predicate",
        predicateType: "isNull",
        variable: "value"
      };
      const context = makeContext({ value: null });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should support accessing different scopes in predicates", () => {
      const condition = {
        type: "predicate",
        predicateType: "isEmpty",
        variable: "output.data"
      };
      const context: EvaluationContext = {
        variables: {},
        input: {},
        output: { data: null }
      };

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });
  });

  describe("Schema Conditions", () => {
    it("should validate string schema", () => {
      const condition = {
        type: "schema",
        variable: "name",
        schema: { type: "string" }
      };
      const context = makeContext({ name: "John" });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should validate number schema", () => {
      const condition = {
        type: "schema",
        variable: "age",
        schema: { type: "number", minimum: 0, maximum: 150 }
      };

      expect(conditionEvaluator.evaluate(condition, makeContext({ age: 25 }))).toBe(true);
      expect(conditionEvaluator.evaluate(condition, makeContext({ age: -1 }))).toBe(false);
      expect(conditionEvaluator.evaluate(condition, makeContext({ age: 200 }))).toBe(false);
    });

    it("should validate array schema", () => {
      const condition = {
        type: "schema",
        variable: "items",
        schema: { type: "array", minItems: 1, maxItems: 10 }
      };

      expect(conditionEvaluator.evaluate(condition, makeContext({ items: [1, 2] }))).toBe(true);
      expect(conditionEvaluator.evaluate(condition, makeContext({ items: [] }))).toBe(false);
    });

    it("should validate object schema", () => {
      const condition = {
        type: "schema",
        variable: "user",
        schema: {
          type: "object",
          properties: {
            name: { type: "string" },
            age: { type: "number" }
          },
          required: ["name"]
        }
      };

      expect(conditionEvaluator.evaluate(condition, makeContext({
        user: { name: "John", age: 30 }
      }))).toBe(true);

      expect(conditionEvaluator.evaluate(condition, makeContext({
        user: { age: 30 }  // Missing required name
      }))).toBe(false);
    });
  });

  describe("Script Conditions", () => {
    it("should execute simple script", () => {
      const condition = {
        type: "script",
        script: "variables.x > 5"
      };
      const context = makeContext({ x: 10 });

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should throw on dangerous patterns in script", () => {
      const condition = {
        type: "script",
        script: "require('fs')"
      };
      const context = makeContext();

      expect(() => conditionEvaluator.evaluate(condition, context)).toThrow();
    });

    it("should block eval in script", () => {
      const condition = {
        type: "script",
        script: "eval('x > 5')"
      };
      const context = makeContext();

      expect(() => conditionEvaluator.evaluate(condition, context)).toThrow();
    });

    it("should NOT cache script results (dependencies unknown)", () => {
      const condition = {
        type: "script",
        script: "variables.x > 0"
      };

      let context = makeContext({ x: 1 });
      const result1 = conditionEvaluator.evaluate(condition, context, "key");
      expect(result1).toBe(true);

      // Change the variable
      context = makeContext({ x: -1 });
      // Without proper caching, this would re-evaluate
      const result2 = conditionEvaluator.evaluate(condition, context, "key");
      // Should reflect the change (not cached)
      expect(result2).toBe(false);
    });
  });

  describe("Default Condition Type", () => {
    it("should treat missing type as expression", () => {
      const condition = {
        expression: "x > 5"
      };
      const context = makeContext({ x: 10 });

      const result = conditionEvaluator.evaluate(condition as any, context);
      expect(result).toBe(true);
    });
  });

  describe("Caching with Cache Key", () => {
    it("should cache result when cache key provided", () => {
      const condition = {
        type: "expression",
        expression: "x > 5"
      };
      let context = makeContext({ x: 10 });

      const result1 = conditionEvaluator.evaluate(condition, context, "cache:key1");
      expect(result1).toBe(true);

      // Second evaluation with same key and context should use cache
      context = makeContext({ x: 10 });
      const result2 = conditionEvaluator.evaluate(condition, context, "cache:key1");
      expect(result2).toBe(true);
    });

    it("should not cache without cache key", () => {
      const condition = {
        type: "expression",
        expression: "x > 5"
      };

      const context1 = makeContext({ x: 10 });
      const result1 = conditionEvaluator.evaluate(condition, context1);

      const context2 = makeContext({ x: 3 });
      const result2 = conditionEvaluator.evaluate(condition, context2);

      expect(result1).toBe(true);
      expect(result2).toBe(false);
    });
  });

  describe("Error Handling and Reporting", () => {
    it("should distinguish between evaluation errors and false results", () => {
      // Valid expression that evaluates to false
      const falseCond = {
        type: "expression",
        expression: "x > 10"
      };
      const context = makeContext({ x: 5 });

      let threw = false;
      try {
        const result = conditionEvaluator.evaluate(falseCond, context);
        expect(result).toBe(false);  // Should return false, not throw
      } catch {
        threw = true;
      }
      expect(threw).toBe(false);

      // Security error should throw
      const securityCond = {
        type: "expression",
        expression: "__proto__"
      };

      expect(() => conditionEvaluator.evaluate(securityCond, context)).toThrow(ExpressionSecurityError);
    });

    it("should re-throw security errors", () => {
      const condition = {
        type: "expression",
        expression: "constructor"
      };
      const context = makeContext();

      expect(() => conditionEvaluator.evaluate(condition, context)).toThrow(ExpressionSecurityError);
    });
  });

  describe("Complex Real-World Scenarios", () => {
    it("should check output data count", () => {
      const condition = {
        type: "expression",
        expression: "output.data.count > 5"
      };
      const context: EvaluationContext = {
        variables: {},
        input: {},
        output: { data: { count: 10 } }
      };

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should handle complex predicate on workflow result", () => {
      const condition = {
        type: "predicate",
        predicateType: "isNotEmpty",
        variable: "output.results"
      };
      const context: EvaluationContext = {
        variables: {},
        input: {},
        output: { results: [1, 2, 3] }
      };

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });

    it("should validate API response shape", () => {
      const condition = {
        type: "schema",
        variable: "output.apiResponse",
        schema: {
          type: "object",
          properties: {
            status: { type: "number" },
            data: { type: "object" }
          },
          required: ["status", "data"]
        }
      };

      const context: EvaluationContext = {
        variables: {},
        input: {},
        output: {
          apiResponse: {
            status: 200,
            data: { items: [] }
          }
        }
      };

      const result = conditionEvaluator.evaluate(condition, context);
      expect(result).toBe(true);
    });
  });

  describe("Condition Type Validation", () => {
    // Issue 7: Unsafe Condition Type Detection - Fixed with validation

    it("should throw TypeError on non-object condition", () => {
      const context = makeContext();
      expect(() => conditionEvaluator.evaluate(null as any, context)).toThrow(TypeError);
      expect(() => conditionEvaluator.evaluate(undefined as any, context)).toThrow(TypeError);
      expect(() => conditionEvaluator.evaluate("invalid" as any, context)).toThrow(TypeError);
      expect(() => conditionEvaluator.evaluate(123 as any, context)).toThrow(TypeError);
    });

    it("should throw TypeError on expression condition with missing field", () => {
      const context = makeContext();
      expect(() => conditionEvaluator.evaluate({
        type: "expression"
        // Missing 'expression' field
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "expression",
        expression: 123  // Wrong type
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "expression",
        expression: ""  // Empty string
      } as any, context)).toThrow(TypeError);
    });

    it("should throw TypeError on predicate condition with missing fields", () => {
      const context = makeContext();
      expect(() => conditionEvaluator.evaluate({
        type: "predicate",
        variable: "x"
        // Missing 'predicateType' field
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "predicate",
        predicateType: "isEmpty"
        // Missing 'variable' field
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "predicate",
        predicateType: "invalidType",  // Invalid predicate type
        variable: "x"
      } as any, context)).toThrow(TypeError);
    });

    it("should throw TypeError on schema condition with missing fields", () => {
      const context = makeContext();
      expect(() => conditionEvaluator.evaluate({
        type: "schema",
        schema: { type: "string" }
        // Missing 'variable' field
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "schema",
        variable: "x"
        // Missing 'schema' field
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "schema",
        variable: "x",
        schema: "invalid"  // Wrong type, should be object
      } as any, context)).toThrow(TypeError);
    });

    it("should throw TypeError on script condition with missing field", () => {
      const context = makeContext();
      expect(() => conditionEvaluator.evaluate({
        type: "script"
        // Missing 'script' field
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "script",
        script: 123  // Wrong type
      } as any, context)).toThrow(TypeError);

      expect(() => conditionEvaluator.evaluate({
        type: "script",
        script: ""  // Empty string
      } as any, context)).toThrow(TypeError);
    });

    it("should throw TypeError on unknown condition type", () => {
      const context = makeContext();
      expect(() => conditionEvaluator.evaluate({
        type: "unknown_type",
        expression: "x > 5"
      } as any, context)).toThrow(TypeError);
    });

    it("should accept valid conditions without error", () => {
      const context = makeContext({ x: 10 });

      // Valid expression condition
      expect(() => conditionEvaluator.evaluate({
        type: "expression",
        expression: "x > 5"
      }, context)).not.toThrow();

      // Valid predicate condition
      expect(() => conditionEvaluator.evaluate({
        type: "predicate",
        predicateType: "isEmpty",
        variable: "x"
      }, context)).not.toThrow();

      // Valid schema condition
      expect(() => conditionEvaluator.evaluate({
        type: "schema",
        variable: "x",
        schema: { type: "number" }
      }, context)).not.toThrow();

      // Valid script condition
      expect(() => conditionEvaluator.evaluate({
        type: "script",
        script: "variables.x > 5"
      }, context)).not.toThrow();
    });

    it("should treat missing type as expression by default", () => {
      const context = makeContext({ x: 10 });
      // When type is missing, should default to "expression"
      const result = conditionEvaluator.evaluate({
        expression: "x > 5"
      } as any, context);
      expect(result).toBe(true);
    });
  });

  describe("Compilation Caching", () => {
    it("should reuse compiled expressions", () => {
      const condition1 = {
        type: "expression",
        expression: "x > 5"
      };

      const condition2 = {
        type: "expression",
        expression: "x > 5"
      };

      const context = makeContext({ x: 10 });

      // Both should compile and cache the same expression
      const stats1 = cacheManager.getStats();
      conditionEvaluator.evaluate(condition1, context);
      const stats2 = cacheManager.getStats();
      expect(stats2.compilation).toBeGreaterThan(stats1.compilation);

      const stats3 = cacheManager.getStats();
      conditionEvaluator.evaluate(condition2, context);
      const stats4 = cacheManager.getStats();
      // Compilation cache should not grow (same expression)
      expect(stats4.compilation).toBe(stats3.compilation);
    });
  });
});
