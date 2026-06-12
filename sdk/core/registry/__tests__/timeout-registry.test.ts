/**
 * TimeoutRegistry Integration Tests
 *
 * Tests for the improved TimeoutRegistry implementation
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TimeoutRegistry } from "../../registry/timeout-registry.js";

describe("TimeoutRegistry Improvements", () => {
  let registry: TimeoutRegistry;

  beforeEach(() => {
    registry = new TimeoutRegistry();
  });

  afterEach(() => {
    registry.cleanupAll();
  });

  describe("Global Statistics", () => {
    it("should track global statistics correctly", () => {
      const exec1 = "exec-1";
      const exec2 = "exec-2";

      // Register timeouts in different executions
      registry.register(exec1, {
        id: "timeout-1",
        duration: 5000,
        onTimeout: async () => {},
        tag: "tag-a",
      });

      registry.register(exec2, {
        id: "timeout-2",
        duration: 5000,
        onTimeout: async () => {},
        tag: "tag-b",
      });

      const stats = registry.getStats();
      expect(stats.totalRegistered).toBe(2);
      expect(stats.activeExecutions).toBe(2);
    });

    it("should update statistics on cleanup", () => {
      const exec1 = "exec-1";

      registry.register(exec1, {
        id: "timeout-1",
        duration: 5000,
        onTimeout: async () => {},
      });

      registry.cleanup(exec1);

      const stats = registry.getStats();
      expect(stats.activeExecutions).toBe(0);
      expect(stats.cancelledCount).toBeGreaterThanOrEqual(1);
    });
  });

  describe("Tag Index", () => {
    it("should track tags across executions", () => {
      const exec1 = "exec-1";
      const exec2 = "exec-2";

      registry.register(exec1, {
        id: "timeout-1",
        duration: 5000,
        onTimeout: async () => {},
        tag: "shared-tag",
      });

      registry.register(exec2, {
        id: "timeout-2",
        duration: 5000,
        onTimeout: async () => {},
        tag: "shared-tag",
      });

      // Both executions should be tracked under the same tag
      expect(registry.getManagerCount()).toBe(2);
    });

    it("should cancel timeouts by tag", () => {
      const exec1 = "exec-1";
      const exec2 = "exec-2";
      const exec3 = "exec-3";

      registry.register(exec1, {
        id: "timeout-1",
        duration: 5000,
        onTimeout: async () => {},
        tag: "tag-a",
      });

      registry.register(exec2, {
        id: "timeout-2",
        duration: 5000,
        onTimeout: async () => {},
        tag: "tag-a",
      });

      registry.register(exec3, {
        id: "timeout-3",
        duration: 5000,
        onTimeout: async () => {},
        tag: "tag-b",
      });

      // Cancel all timeouts with tag-a
      registry.cancelByTag("tag-a");

      // Verify that tag-a timeouts are cancelled (managers cleared)
      const stats = registry.getStats();
      expect(stats.cancelledCount).toBeGreaterThanOrEqual(2);
    });

    it("should handle non-existent tag gracefully", () => {
      // Should not throw error
      expect(() => registry.cancelByTag("non-existent")).not.toThrow();
    });
  });

  describe("Error Handling", () => {
    it("should handle cleanup errors gracefully", () => {
      const exec1 = "exec-1";

      registry.register(exec1, {
        id: "timeout-1",
        duration: 5000,
        onTimeout: async () => {},
      });

      // Cleanup should not throw even if there are issues
      expect(() => registry.cleanup(exec1)).not.toThrow();
    });

    it("should validate configuration", () => {
      expect(() => {
        new TimeoutRegistry({
          maxTimeoutsPerExecution: 0,
        });
      }).toThrow("maxTimeoutsPerExecution must be positive");
    });
  });

  describe("Configuration", () => {
    it("should use custom maxTimeoutsPerExecution", () => {
      const customRegistry = new TimeoutRegistry({
        defaultManagerConfig: {
          maxTimeoutsPerExecution: 10,
        },
      });

      const manager = customRegistry.getManager("test-exec");

      // Register up to the limit
      for (let i = 0; i < 10; i++) {
        manager.register({
          id: `timeout-${i}`,
          duration: 5000,
          onTimeout: async () => {},
        });
      }

      // The 11th registration should fail
      expect(() => {
        manager.register({
          id: "timeout-10",
          duration: 5000,
          onTimeout: async () => {},
        });
      }).toThrow("Maximum number of timeouts (10) reached");

      customRegistry.cleanupAll();
    });
  });
});
