import { describe, it, expect, beforeEach } from "vitest";
import { DependencyManager, createDependencyManager } from "../dependency-tracker.js";
import type { EvaluationContext } from "@wf-agent/types";

function makeContext(variables: Record<string, unknown>): EvaluationContext {
  return { variables, input: {}, output: {} };
}

describe("DependencyManager", () => {
  let dm: DependencyManager;

  beforeEach(() => {
    dm = new DependencyManager();
  });

  it("should register an expression and return tracked info", () => {
    const ctx = makeContext({ count: 5 });
    const tracked = dm.register("expr1", "count > 3", ctx);
    expect(tracked.expression).toBe("count > 3");
    expect(tracked.dependencies).toContain("count");
    expect(tracked.lastResult).toBe(true);
    expect(tracked.lastEvaluatedAt).toBeGreaterThan(0);
  });

  it("should cache result when dependencies have not changed", () => {
    const ctx = makeContext({ count: 5 });
    dm.register("expr1", "count > 3", ctx);
    const result1 = dm.evaluateIfChanged("expr1", ctx);
    const result2 = dm.evaluateIfChanged("expr1", ctx);
    expect(result1).toBe(true);
    expect(result2).toBe(true);
  });

  it("should re-evaluate when dependency value changes", () => {
    const ctx1 = makeContext({ count: 5 });
    dm.register("expr1", "count > 3", ctx1);
    expect(dm.evaluateIfChanged("expr1", ctx1)).toBe(true);

    const ctx2 = makeContext({ count: 1 });
    expect(dm.evaluateIfChanged("expr1", ctx2)).toBe(false);
  });

  it("should re-evaluate when dependency is first seen", () => {
    const ctx = makeContext({ x: 10 });
    dm.register("expr1", "x > 5", ctx);
    expect(dm.evaluateIfChanged("expr1", ctx)).toBe(true);
  });

  it("should detect dependency changes", () => {
    const ctx = makeContext({ a: 1 });
    dm.register("expr1", "a == 1", ctx);
    expect(dm.hasDependenciesChanged("expr1", makeContext({ a: 2 }))).toBe(true);
    expect(dm.hasDependenciesChanged("expr1", makeContext({ a: 1 }))).toBe(false);
  });

  it("should throw for unregistered expression on evaluateIfChanged", () => {
    const ctx = makeContext({});
    expect(() => dm.evaluateIfChanged("unknown", ctx)).toThrow("Expression not found: unknown");
  });

  it("should throw for unregistered expression on hasDependenciesChanged", () => {
    const ctx = makeContext({});
    expect(() => dm.hasDependenciesChanged("unknown", ctx)).toThrow(
      "Expression not found: unknown",
    );
  });

  it("should unregister expressions", () => {
    const ctx = makeContext({ count: 5 });
    dm.register("expr1", "count > 3", ctx);
    dm.unregister("expr1");
    expect(() => dm.evaluateIfChanged("expr1", ctx)).toThrow("Expression not found: expr1");
  });

  it("should clear all tracked data", () => {
    const ctx = makeContext({ count: 5 });
    dm.register("expr1", "count > 3", ctx);
    dm.clear();
    expect(() => dm.evaluateIfChanged("expr1", ctx)).toThrow();
  });

  it("should get tracked expression info", () => {
    const ctx = makeContext({ x: 1 });
    dm.register("key1", "x == 1", ctx);
    const info = dm.getTrackedExpression("key1");
    expect(info).toBeDefined();
    expect(info!.expression).toBe("x == 1");
  });

  it("should return undefined for non-existent tracked expression", () => {
    expect(dm.getTrackedExpression("nonexistent")).toBeUndefined();
  });

  it("should handle ternary expression dependencies", () => {
    const ctx = makeContext({ a: true, b: 1, c: 2 });
    const tracked = dm.register("tern", "a ? b : c", ctx);
    expect(tracked.dependencies).toContain("a");
    expect(tracked.dependencies).toContain("b");
    expect(tracked.dependencies).toContain("c");
  });

  it("should track member access dependencies", () => {
    const ctx = makeContext({ user: { name: "Alice", age: 30 } });
    const tracked = dm.register("member", "user.name == 'Alice'", ctx);
    expect(tracked.dependencies).toContain("user.name");
  });

  it("should detect nested dependency changes", () => {
    const ctx1 = makeContext({ user: { name: "Alice" } });
    dm.register("expr2", "user.name == 'Alice'", ctx1);
    expect(dm.evaluateIfChanged("expr2", ctx1)).toBe(true);

    const ctx2 = makeContext({ user: { name: "Bob" } });
    expect(dm.evaluateIfChanged("expr2", ctx2)).toBe(false);
  });
});

describe("createDependencyManager", () => {
  it("should create a new DependencyManager instance", () => {
    const dm = createDependencyManager();
    expect(dm).toBeInstanceOf(DependencyManager);
  });

  it("should work with register and evaluate", () => {
    const dm = createDependencyManager();
    const ctx = makeContext({ x: 5 });
    dm.register("test", "x > 0", ctx);
    expect(dm.evaluateIfChanged("test", ctx)).toBe(true);
  });
});
