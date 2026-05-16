/**
 * Timeout Manager Tests
 * 
 * Basic tests to verify TimeoutManager functionality.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TimeoutManager } from "../timeout-manager.js";
import type { TimeoutRegistration } from "../../types/timeout.js";

describe("TimeoutManager", () => {
  let manager: TimeoutManager;

  beforeEach(() => {
    manager = new TimeoutManager();
  });

  afterEach(() => {
    manager.clear();
  });

  describe("register", () => {
    it("should register a timeout successfully", () => {
      const handle = manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
      });

      expect(handle).toBeDefined();
      expect(handle.id).toBe("test-1");
      expect(handle.isActive()).toBe(true);
    });

    it("should throw error for duplicate ID", () => {
      manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
      });

      expect(() => {
        manager.register({
          id: "test-1",
          duration: 5000,
          onTimeout: () => {},
        });
      }).toThrow("already exists");
    });

    it("should throw error for invalid duration", () => {
      expect(() => {
        manager.register({
          id: "test-1",
          duration: -1000,
          onTimeout: () => {},
        });
      }).toThrow("must be positive");
    });
  });

  describe("cancel", () => {
    it("should cancel an active timeout", () => {
      const handle = manager.register({
        id: "test-1",
        duration: 60000,
        onTimeout: () => {},
      });

      expect(handle.isActive()).toBe(true);
      handle.cancel();
      expect(handle.isActive()).toBe(false);
    });

    it("should be idempotent", () => {
      const handle = manager.register({
        id: "test-1",
        duration: 60000,
        onTimeout: () => {},
      });

      handle.cancel();
      handle.cancel(); // Should not throw
      expect(handle.isActive()).toBe(false);
    });
  });

  describe("getRemainingTime", () => {
    it("should return remaining time for active timeout", () => {
      const handle = manager.register({
        id: "test-1",
        duration: 10000,
        onTimeout: () => {},
      });

      const remaining = handle.getRemainingTime();
      expect(remaining).toBeGreaterThan(0);
      expect(remaining).toBeLessThanOrEqual(10000);
    });

    it("should return 0 for cancelled timeout", () => {
      const handle = manager.register({
        id: "test-1",
        duration: 10000,
        onTimeout: () => {},
      });

      handle.cancel();
      expect(handle.getRemainingTime()).toBe(0);
    });
  });

  describe("refresh", () => {
    it("should refresh an active timeout", () => {
      const handle = manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
      });

      const initialRemaining = handle.getRemainingTime();

      // Wait a bit
      setTimeout(() => {
        const beforeRefresh = handle.getRemainingTime();
        expect(beforeRefresh).toBeLessThan(initialRemaining);

        // Refresh
        manager.refresh(handle);
        const afterRefresh = handle.getRemainingTime();
        expect(afterRefresh).toBeGreaterThan(beforeRefresh);
      }, 1000);
    });
  });

  describe("timeout expiration", () => {
    it("should call onTimeout callback when timeout expires", async () => {
      let timeoutCalled = false;

      const handle = manager.register({
        id: "test-1",
        duration: 100,
        onTimeout: () => {
          timeoutCalled = true;
        },
      });

      // Wait for timeout
      await new Promise((resolve) => setTimeout(resolve, 200));

      expect(timeoutCalled).toBe(true);
      expect(handle.isActive()).toBe(false);
    });
  });

  describe("warning", () => {
    it("should call onWarning callback when warning threshold is reached", async () => {
      let warningCalled = false;

      const handle = manager.register({
        id: "test-1",
        duration: 500,
        warningThreshold: 200,
        onWarning: () => {
          warningCalled = true;
        },
        onTimeout: () => {},
      });

      // Wait for warning (should trigger at 300ms)
      await new Promise((resolve) => setTimeout(resolve, 350));

      expect(warningCalled).toBe(true);
      expect(handle.isActive()).toBe(true); // Still active until full timeout
    });
  });

  describe("statistics", () => {
    it("should track statistics correctly", () => {
      manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
        tag: "llm",
      });

      manager.register({
        id: "test-2",
        duration: 5000,
        onTimeout: () => {},
        tag: "tool",
      });

      const stats = manager.getStats();

      expect(stats.activeTimeouts).toBe(2);
      expect(stats.totalRegistered).toBe(2);
      expect(stats.byTag["llm"]).toBe(1);
      expect(stats.byTag["tool"]).toBe(1);
    });
  });

  describe("StateManager interface", () => {
    it("should implement size() correctly", () => {
      expect(manager.size()).toBe(0);

      manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
      });

      expect(manager.size()).toBe(1);
    });

    it("should implement isEmpty() correctly", () => {
      expect(manager.isEmpty()).toBe(true);

      manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
      });

      expect(manager.isEmpty()).toBe(false);
    });

    it("should implement clear() correctly", () => {
      manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
      });

      manager.register({
        id: "test-2",
        duration: 5000,
        onTimeout: () => {},
      });

      expect(manager.size()).toBe(2);

      manager.clear();

      expect(manager.size()).toBe(0);
      expect(manager.isEmpty()).toBe(true);
    });

    it("should implement serialize() correctly", () => {
      manager.register({
        id: "test-1",
        duration: 5000,
        onTimeout: () => {},
        metadata: { key: "value" },
      });

      const snapshot = manager.serialize();

      expect(snapshot.version).toBe(1);
      expect(snapshot.timeouts).toHaveLength(1);
      expect(snapshot.timeouts[0].id).toBe("test-1");
      expect(snapshot.timeouts[0].metadata).toEqual({ key: "value" });
    });

    it("should implement restore() correctly", () => {
      const snapshot = {
        version: 1,
        timestamp: Date.now(),
        timeouts: [
          {
            id: "test-1",
            startTime: Date.now(),
            duration: 5000,
            status: "active" as const,
            warningEmitted: false,
            metadata: { restored: true },
          },
        ],
      };

      manager.restore(snapshot);

      expect(manager.size()).toBe(1);
    });
  });
});
