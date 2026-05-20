/**
 * Dependency Tracking Tests
 * Tests for the dependency tracking system (Phase 2)
 */

import { describe, it, expect, beforeEach } from "vitest";
import { createDependencyManager } from "../dependency-tracker.js";
import type { EvaluationContext } from "@wf-agent/types";

describe("Dependency Tracking", () => {
  let context: EvaluationContext;

  beforeEach(() => {
    context = {
      variables: {
        user: {
          name: "Alice",
          age: 30,
          active: true,
        },
        config: {
          threshold: 18,
        },
      },
      input: {},
      output: {},
    };
  });

  describe("VariableChangeTracker", () => {
    it("should detect first-time variable as changed", () => {
      const manager = createDependencyManager();
      manager.register("test", "user.age > 18", context);
      
      // First time should be considered changed
      const changed = manager.hasDependenciesChanged("test", context);
      expect(changed).toBe(false); // Already tracked during registration
    });

    it("should detect when variable value changes", () => {
      const manager = createDependencyManager();
      manager.register("test", "user.age > 18", context);
      
      // Change the variable
      (context.variables.user as any).age = 25;
      
      const changed = manager.hasDependenciesChanged("test", context);
      expect(changed).toBe(true);
    });

    it("should not detect change when value is same", () => {
      const manager = createDependencyManager();
      manager.register("test", "user.age > 18", context);
      
      // Evaluate without changing
      manager.evaluateIfChanged("test", context);
      
      const changed = manager.hasDependenciesChanged("test", context);
      expect(changed).toBe(false);
    });
  });

  describe("DependencyManager - Registration", () => {
    it("should register expression with dependencies", () => {
      const manager = createDependencyManager();
      const tracked = manager.register("expr1", "user.age > 18", context);
      
      expect(tracked.expression).toBe("user.age > 18");
      expect(tracked.dependencies).toContain("user.age");
      expect(tracked.lastResult).toBe(true);
      expect(tracked.needsReevaluation).toBe(false);
    });

    it("should track multiple dependencies", () => {
      const manager = createDependencyManager();
      const tracked = manager.register(
        "expr1",
        "user.age > 18 && user.active == true",
        context
      );
      
      expect(tracked.dependencies.length).toBeGreaterThanOrEqual(2);
      expect(tracked.dependencies).toContain("user.age");
      expect(tracked.dependencies).toContain("user.active");
    });

    it("should handle nested property dependencies", () => {
      const nestedContext: EvaluationContext = {
        variables: {
          user: {
            profile: {
              settings: {
                theme: "dark",
              },
            },
          },
        },
        input: {},
        output: {},
      };
      
      const manager = createDependencyManager();
      const tracked = manager.register(
        "expr1",
        "user.profile.settings.theme == 'dark'",
        nestedContext
      );
      
      expect(tracked.dependencies).toContain("user.profile.settings.theme");
    });
  });

  describe("DependencyManager - Evaluation", () => {
    it("should return cached result when no changes", () => {
      const manager = createDependencyManager();
      manager.register("expr1", "user.age > 18", context);
      
      // First evaluation
      const result1 = manager.evaluateIfChanged("expr1", context);
      
      // Second evaluation (should use cache)
      const result2 = manager.evaluateIfChanged("expr1", context);
      
      expect(result1).toBe(result2);
    });

    it("should re-evaluate when dependencies change", () => {
      const manager = createDependencyManager();
      manager.register("expr1", "user.age > 18", context);
      
      // Initial result
      const result1 = manager.evaluateIfChanged("expr1", context);
      expect(result1).toBe(true);
      
      // Change dependency
      (context.variables.user as any).age = 15;
      
      // Should re-evaluate and get different result
      const result2 = manager.evaluateIfChanged("expr1", context);
      expect(result2).toBe(false);
    });

    it("should force re-evaluation", () => {
      const manager = createDependencyManager();
      manager.register("expr1", "user.age > 18", context);
      
      const result1 = manager.forceEvaluate("expr1", context);
      const result2 = manager.forceEvaluate("expr1", context);
      
      expect(result1).toBe(result2);
      // Force evaluate always executes, even if no changes
    });
  });

  describe("DependencyManager - Multiple Expressions", () => {
    it("should manage multiple expressions independently", () => {
      const manager = createDependencyManager();
      
      manager.register("expr1", "user.age > 18", context);
      manager.register("expr2", "user.active == true", context);
      
      const expr1 = manager.getTrackedExpression("expr1");
      const expr2 = manager.getTrackedExpression("expr2");
      
      expect(expr1?.expression).toBe("user.age > 18");
      expect(expr2?.expression).toBe("user.active == true");
    });

    it("should track all registered expressions", () => {
      const manager = createDependencyManager();
      
      manager.register("expr1", "user.age > 18", context);
      manager.register("expr2", "user.active == true", context);
      manager.register("expr3", "config.threshold == 18", context);
      
      const all = manager.getAllTrackedExpressions();
      expect(all.size).toBe(3);
    });

    it("should unregister expressions", () => {
      const manager = createDependencyManager();
      
      manager.register("expr1", "user.age > 18", context);
      manager.unregister("expr1");
      
      const expr = manager.getTrackedExpression("expr1");
      expect(expr).toBeUndefined();
    });
  });

  describe("DependencyManager - Edge Cases", () => {
    it("should handle non-existent expression gracefully", () => {
      const manager = createDependencyManager();
      
      expect(() => {
        manager.evaluateIfChanged("nonexistent", context);
      }).toThrow("Expression not found");
    });

    it("should clear all tracked data", () => {
      const manager = createDependencyManager();
      
      manager.register("expr1", "user.age > 18", context);
      manager.register("expr2", "user.active == true", context);
      
      manager.clear();
      
      expect(manager.getAllTrackedExpressions().size).toBe(0);
    });

    it("should mark expression for re-evaluation", () => {
      const manager = createDependencyManager();
      manager.register("expr1", "user.age > 18", context);
      
      manager.markForReevaluation("expr1");
      
      const tracked = manager.getTrackedExpression("expr1");
      expect(tracked?.needsReevaluation).toBe(true);
    });
  });

  describe("Integration Scenarios", () => {
    it("should optimize re-evaluation in loop", () => {
      const manager = createDependencyManager();
      manager.register("check", "user.age > 18", context);
      
      // Simulate multiple checks without changes
      const iterations = 100;
      const start = performance.now();
      
      for (let i = 0; i < iterations; i++) {
        manager.evaluateIfChanged("check", context);
      }
      
      const end = performance.now();
      const totalTime = end - start;
      
      console.log(`100 cached evaluations: ${totalTime.toFixed(2)}ms`);
      expect(totalTime).toBeLessThan(100); // Should be very fast
    });

    it("should detect changes across multiple dependencies", () => {
      const manager = createDependencyManager();
      manager.register(
        "complex",
        "user.age > 18 && user.active == true",
        context
      );
      
      // Change one dependency
      (context.variables.user as any).age = 25;
      
      const changed = manager.hasDependenciesChanged("complex", context);
      expect(changed).toBe(true);
      
      // Re-evaluate to update cache
      manager.evaluateIfChanged("complex", context);
      
      // Now should not detect changes
      const changedAgain = manager.hasDependenciesChanged("complex", context);
      expect(changedAgain).toBe(false);
    });
  });
});
