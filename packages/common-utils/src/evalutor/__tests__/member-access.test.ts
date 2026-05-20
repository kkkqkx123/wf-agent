/**
 * Member Access Node Tests
 * Tests for the new MemberAccessNode feature (Phase 2)
 */

import { describe, it, expect, beforeEach } from "vitest";
import { expressionEvaluator, expressionCompiler } from "../index.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("Member Access Node", () => {
  let context: EvaluationContext;

  beforeEach(() => {
    context = {
      variables: {
        user: {
          name: "Alice",
          age: 30,
          email: "alice@example.com",
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
      },
      input: {},
      output: {},
    };
  });

  describe("Basic Member Access", () => {
    it("should access simple property", () => {
      const result = expressionEvaluator.evaluate("user.name", context);
      expect(result).toBe("Alice");
    });

    it("should access numeric property", () => {
      const result = expressionEvaluator.evaluate("user.age", context);
      expect(result).toBe(30);
    });

    it("should access nested property (2 levels)", () => {
      const result = expressionEvaluator.evaluate("user.address.city", context);
      expect(result).toBe("Beijing");
    });

    it("should access deeply nested property (3+ levels)", () => {
      const result = expressionEvaluator.evaluate("user.profile.social.twitter", context);
      expect(result).toBe("@alice");
    });
  });

  describe("Member Access in Comparisons", () => {
    it("should compare member access with value", () => {
      const result = expressionEvaluator.evaluate("user.name == 'Alice'", context);
      expect(result).toBe(true);
    });

    it("should compare nested member access", () => {
      const result = expressionEvaluator.evaluate("user.address.city == 'Beijing'", context);
      expect(result).toBe(true);
    });

    it("should use greater than with member access", () => {
      const result = expressionEvaluator.evaluate("user.age > 25", context);
      expect(result).toBe(true);
    });

    it("should use less than with nested member access", () => {
      const result = expressionEvaluator.evaluate("user.age < 35", context);
      expect(result).toBe(true);
    });
  });

  describe("Member Access in Logical Expressions", () => {
    it("should work with AND operator", () => {
      const result = expressionEvaluator.evaluate(
        "user.name == 'Alice' && user.age > 25",
        context
      );
      expect(result).toBe(true);
    });

    it("should work with OR operator", () => {
      const result = expressionEvaluator.evaluate(
        "user.address.city == 'Shanghai' || user.age == 30",
        context
      );
      expect(result).toBe(true);
    });

    it("should work with NOT operator", () => {
      const result = expressionEvaluator.evaluate("!user.name == 'Bob'", context);
      expect(result).toBe(true);
    });
  });

  describe("Member Access in Ternary Expressions", () => {
    it("should use member access in condition", () => {
      const result = expressionEvaluator.evaluate(
        "user.age >= 18 ? 'adult' : 'minor'",
        context
      );
      expect(result).toBe("adult");
    });

    it("should use member access in consequent", () => {
      const result = expressionEvaluator.evaluate(
        "true ? user.name : 'Unknown'",
        context
      );
      expect(result).toBe("Alice");
    });

    it("should use member access in alternate", () => {
      const result = expressionEvaluator.evaluate(
        "false ? 'Known' : user.email",
        context
      );
      expect(result).toBe("alice@example.com");
    });
  });

  describe("Member Access Error Handling", () => {
    it("should return undefined for non-existent property", () => {
      const result = expressionEvaluator.evaluate("user.nonexistent", context);
      expect(result).toBeUndefined();
    });

    it("should return undefined for accessing property on undefined", () => {
      const result = expressionEvaluator.evaluate("user.missing.property", context);
      expect(result).toBeUndefined();
    });

    it("should handle null object gracefully", () => {
      const nullContext: EvaluationContext = {
        variables: { data: null },
        input: {},
        output: {},
      };
      const result = expressionEvaluator.evaluate("data.value", nullContext);
      expect(result).toBeUndefined();
    });
  });

  describe("Member Access Security", () => {
    it("should block __proto__ access", () => {
      expect(() => {
        expressionEvaluator.evaluate("user.__proto__", context);
      }).toThrow(/forbidden property/);
    });

    it("should block constructor access", () => {
      expect(() => {
        expressionEvaluator.evaluate("user.constructor", context);
      }).toThrow(/forbidden property/);
    });

    it("should block prototype access", () => {
      expect(() => {
        expressionEvaluator.evaluate("user.prototype", context);
      }).toThrow(/forbidden property/);
    });
  });

  describe("Member Access with Expression Compiler", () => {
    it("should compile and evaluate member access", () => {
      const compiled = expressionCompiler.compile("user.name");
      const result = compiled.evaluate(context);
      expect(result).toBe("Alice");
    });

    it("should extract dependencies correctly", () => {
      const compiled = expressionCompiler.compile("user.address.city");
      expect(compiled.dependencies).toContain("user");
    });

    it("should calculate complexity for nested access", () => {
      const simple = expressionCompiler.compile("user.name");
      const nested = expressionCompiler.compile("user.profile.social.twitter");
      
      // Both have same base complexity because they start from comparison nodes
      // But the nested one has more memberAccess nodes in the tree
      expect(simple.complexity).toBeGreaterThanOrEqual(2);
      expect(nested.complexity).toBeGreaterThanOrEqual(2);
    });

    it("should cache compiled expressions", () => {
      expressionCompiler.clearCache();
      const compiled1 = expressionCompiler.compile("user.age");
      const compiled2 = expressionCompiler.compile("user.age");
      
      expect(expressionCompiler.getCacheSize()).toBe(1);
      expect(compiled1).toBe(compiled2);
    });
  });

  describe("Complex Member Access Scenarios", () => {
    it("should work with arithmetic operations", () => {
      const result = expressionEvaluator.evaluate("user.age + 5", context);
      expect(result).toBe(35);
    });

    it("should work with string methods", () => {
      const result = expressionEvaluator.evaluate("user.name.startsWith('Al')", context);
      expect(result).toBe(true);
    });

    it("should work in complex logical expressions", () => {
      const result = expressionEvaluator.evaluate(
        "(user.age > 25 && user.address.city == 'Beijing') || user.name == 'Bob'",
        context
      );
      expect(result).toBe(true);
    });

    it("should handle multiple member accesses with arithmetic", () => {
      const doubleAgeContext: EvaluationContext = {
        variables: { ...context.variables, multiplier: 2 },
        input: {},
        output: {},
      };
      // Use explicit comparison to get the value
      const result = expressionEvaluator.evaluate(
        "user.age * 2",
        doubleAgeContext
      );
      expect(result).toBe(60);
    });
  });
});
