import { describe, it, expect, beforeEach } from "vitest";
import { ConditionEvaluator } from "../condition-evaluator.js";
import { ExpressionSecurityError } from "@wf-agent/types";
import type { Condition, EvaluationContext } from "@wf-agent/types";

function makeContext(variables: Record<string, unknown> = {}): EvaluationContext {
  return { variables, input: {}, output: {} };
}

function makeCondition(expression: string): Condition {
  return { expression };
}

describe("ConditionEvaluator", () => {
  let evaluator: ConditionEvaluator;

  beforeEach(() => {
    evaluator = new ConditionEvaluator();
  });

  it("should return true for a true condition", () => {
    const condition = makeCondition("5 > 3");
    const result = evaluator.evaluate(condition, makeContext());
    expect(result).toBe(true);
  });

  it("should return false for a false condition", () => {
    const condition = makeCondition("5 < 3");
    const result = evaluator.evaluate(condition, makeContext());
    expect(result).toBe(false);
  });

  it("should evaluate variable-based conditions", () => {
    const condition = makeCondition("count >= 10");
    const ctx = makeContext({ count: 15 });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should evaluate string comparisons", () => {
    const condition = makeCondition("name == 'Alice'");
    const ctx = makeContext({ name: "Alice" });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should evaluate logical AND conditions", () => {
    const condition = makeCondition("a > 0 && b < 10");
    const ctx = makeContext({ a: 5, b: 8 });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should evaluate logical OR conditions", () => {
    const condition = makeCondition("x == 1 || x == 2");
    const ctx = makeContext({ x: 2 });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should evaluate NOT conditions", () => {
    const condition = makeCondition("!flag");
    const ctx = makeContext({ flag: false });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should evaluate ternary conditions", () => {
    const condition = makeCondition("a ? b > 5 : b < 10");
    const ctx = makeContext({ a: true, b: 7 });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should return false when dependency path is missing", () => {
    const condition = makeCondition("missing.path > 5");
    const ctx = makeContext({});
    expect(evaluator.evaluate(condition, ctx)).toBe(false);
  });

  it("should return false when input dependency is missing", () => {
    const condition = makeCondition("input.data.count > 5");
    const ctx = makeContext({});
    expect(evaluator.evaluate(condition, ctx)).toBe(false);
  });

  it("should return false when output dependency is missing", () => {
    const condition = makeCondition("output.result.status == 'done'");
    const ctx = makeContext({});
    expect(evaluator.evaluate(condition, ctx)).toBe(false);
  });

  it("should return false when condition expression evaluates to false", () => {
    const condition = makeCondition("1 == 2");
    expect(evaluator.evaluate(condition, makeContext())).toBe(false);
  });

  it("should throw when condition has no expression", () => {
    const invalidCondition = {} as Condition;
    expect(() => evaluator.evaluate(invalidCondition, makeContext())).toThrow(ExpressionSecurityError);
  });

  it("should rethrow ExpressionSecurityError", () => {
    // Expression containing forbidden property __proto__ triggers ExpressionSecurityError
    const ctx = makeContext({ obj: {} });
    expect(() => evaluator.evaluate(makeCondition("obj.__proto__"), ctx)).toThrow(ExpressionSecurityError);
  });

  it("should rethrow ExpressionSecurityError for empty expression", () => {
    expect(() => evaluator.evaluate(makeCondition(""), makeContext())).toThrow(ExpressionSecurityError);
  });

  it("should handle runtime errors gracefully and return false", () => {
    const condition = makeCondition("a / 0 > 1");
    const ctx = makeContext({ a: 5 });
    const result = evaluator.evaluate(condition, ctx);
    expect(result).toBe(false);
  });

  it("should evaluate complex nested conditions", () => {
    const condition = makeCondition("user.age >= 18 && user.role == 'admin'");
    const ctx = makeContext({ user: { age: 25, role: "admin" } });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should evaluate contains condition", () => {
    const condition = makeCondition("name contains 'Smith'");
    const ctx = makeContext({ name: "John Smith" });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });

  it("should evaluate 'in' condition with arrays", () => {
    const condition = makeCondition("role in ['admin', 'moderator']");
    const ctx = makeContext({ role: "admin" });
    expect(evaluator.evaluate(condition, ctx)).toBe(true);
  });
});