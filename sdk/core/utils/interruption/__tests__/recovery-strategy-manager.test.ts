/**
 * Unit Tests for Recovery Strategy Manager
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  RecoveryStrategyManager,
  createAutoSaveStrategy,
  type RecoveryContext,
} from "../recovery-strategy-manager.js";

describe("RecoveryStrategyManager", () => {
  let manager: RecoveryStrategyManager;

  beforeEach(() => {
    manager = new RecoveryStrategyManager();
  });

  describe("register / hasStrategy / getRegisteredTypes", () => {
    it("should register a strategy for an execution type", () => {
      const strategy = { beforeInterrupt: vi.fn() };
      manager.register("workflow", strategy);

      expect(manager.hasStrategy("workflow")).toBe(true);
      expect(manager.getRegisteredTypes()).toEqual(["workflow"]);
    });

    it("should overwrite existing strategy for the same type", () => {
      const s1 = { beforeInterrupt: vi.fn() };
      const s2 = { beforeInterrupt: vi.fn() };
      manager.register("workflow", s1);
      manager.register("workflow", s2);

      expect(manager.getRegisteredTypes()).toEqual(["workflow"]);
    });
  });

  describe("unregister", () => {
    it("should unregister a strategy", () => {
      manager.register("workflow", {});
      manager.unregister("workflow");

      expect(manager.hasStrategy("workflow")).toBe(false);
    });

    it("should not throw when unregistering non-existent type", () => {
      expect(() => manager.unregister("non-existent")).not.toThrow();
    });
  });

  describe("beforeInterrupt", () => {
    it("should call the strategy's beforeInterrupt callback", async () => {
      const beforeInterrupt = vi.fn();
      manager.register("workflow", { beforeInterrupt });

      const context: RecoveryContext = {
        executionId: "exec-1",
        state: { foo: "bar" },
      };

      await manager.beforeInterrupt("workflow", "PAUSE", context);

      expect(beforeInterrupt).toHaveBeenCalledWith("PAUSE", context);
    });

    it("should not throw when no strategy registered", async () => {
      await expect(
        manager.beforeInterrupt("workflow", "PAUSE", {
          executionId: "e1",
          state: {},
        }),
      ).resolves.toBeUndefined();
    });

    it("should not throw when strategy has no beforeInterrupt method", async () => {
      manager.register("workflow", {});

      await expect(
        manager.beforeInterrupt("workflow", "PAUSE", {
          executionId: "e1",
          state: {},
        }),
      ).resolves.toBeUndefined();
    });

    it("should swallow errors from strategy callback", async () => {
      const beforeInterrupt = vi.fn().mockRejectedValue(new Error("strategy error"));
      manager.register("workflow", { beforeInterrupt });

      await expect(
        manager.beforeInterrupt("workflow", "PAUSE", {
          executionId: "e1",
          state: {},
        }),
      ).resolves.toBeUndefined();
    });
  });

  describe("beforeResume / afterResume", () => {
    it("should call beforeResume on registered strategy", async () => {
      const beforeResume = vi.fn();
      manager.register("agent-loop", { beforeResume });

      const context: RecoveryContext = { executionId: "e1", state: {} };
      await manager.beforeResume("agent-loop", context);

      expect(beforeResume).toHaveBeenCalledWith(context);
    });

    it("should call afterResume on registered strategy", async () => {
      const afterResume = vi.fn();
      manager.register("agent-loop", { afterResume });

      const context: RecoveryContext = { executionId: "e1", state: {} };
      await manager.afterResume("agent-loop", context);

      expect(afterResume).toHaveBeenCalledWith(context);
    });

    it("should swallow errors in beforeResume", async () => {
      const beforeResume = vi.fn().mockRejectedValue(new Error("resume error"));
      manager.register("agent-loop", { beforeResume });

      await expect(
        manager.beforeResume("agent-loop", { executionId: "e1", state: {} }),
      ).resolves.toBeUndefined();
    });

    it("should swallow errors in afterResume", async () => {
      const afterResume = vi.fn().mockRejectedValue(new Error("after error"));
      manager.register("agent-loop", { afterResume });

      await expect(
        manager.afterResume("agent-loop", { executionId: "e1", state: {} }),
      ).resolves.toBeUndefined();
    });
  });

  describe("clear", () => {
    it("should clear all registered strategies", () => {
      manager.register("workflow", {});
      manager.register("agent-loop", {});
      manager.clear();

      expect(manager.getRegisteredTypes()).toEqual([]);
    });
  });
});

describe("createAutoSaveStrategy", () => {
  it("should save checkpoint on PAUSE interrupt", async () => {
    const saveCheckpoint = vi.fn();
    const loadCheckpoint = vi.fn();
    const strategy = createAutoSaveStrategy({ saveCheckpoint, loadCheckpoint });

    const context: RecoveryContext = {
      executionId: "exec-1",
      state: { data: "test" },
    };

    await strategy.beforeInterrupt!("PAUSE", context);

    expect(saveCheckpoint).toHaveBeenCalledWith("exec-1", { data: "test" });
  });

  it("should not save checkpoint on STOP interrupt", async () => {
    const saveCheckpoint = vi.fn();
    const strategy = createAutoSaveStrategy({
      saveCheckpoint,
      loadCheckpoint: vi.fn(),
    });

    await strategy.beforeInterrupt!("STOP", {
      executionId: "e1",
      state: {},
    });

    expect(saveCheckpoint).not.toHaveBeenCalled();
  });

  it("should restore checkpoint on beforeResume", async () => {
    const loadCheckpoint = vi.fn().mockResolvedValue({ state: { restored: true } });
    const strategy = createAutoSaveStrategy({
      saveCheckpoint: vi.fn(),
      loadCheckpoint,
    });

    const context: RecoveryContext = {
      executionId: "exec-1",
      state: { data: "original" },
    };

    await strategy.beforeResume!(context);

    expect(loadCheckpoint).toHaveBeenCalledWith("exec-1");
    expect(context.state).toEqual({ data: "original", restored: true });
  });

  it("should handle missing checkpoint gracefully", async () => {
    const loadCheckpoint = vi.fn().mockResolvedValue(null);
    const strategy = createAutoSaveStrategy({
      saveCheckpoint: vi.fn(),
      loadCheckpoint,
    });

    const context: RecoveryContext = {
      executionId: "e1",
      state: { data: "original" },
    };

    await strategy.beforeResume!(context);

    expect(context.state).toEqual({ data: "original" });
  });

  it("should swallow save errors", async () => {
    const saveCheckpoint = vi.fn().mockRejectedValue(new Error("save failed"));
    const strategy = createAutoSaveStrategy({
      saveCheckpoint,
      loadCheckpoint: vi.fn(),
    });

    await expect(
      strategy.beforeInterrupt!("PAUSE", { executionId: "e1", state: {} }),
    ).resolves.toBeUndefined();
  });

  it("should swallow load errors", async () => {
    const loadCheckpoint = vi.fn().mockRejectedValue(new Error("load failed"));
    const strategy = createAutoSaveStrategy({
      saveCheckpoint: vi.fn(),
      loadCheckpoint,
    });

    await expect(strategy.beforeResume!({ executionId: "e1", state: {} })).resolves.toBeUndefined();
  });
});
