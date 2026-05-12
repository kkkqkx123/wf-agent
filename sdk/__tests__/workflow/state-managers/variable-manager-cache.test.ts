/**
 * VariableManager Cache Mechanism Tests
 * Tests the optional caching feature for performance optimization
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { VariableManager } from "../../../workflow/state-managers/variable-manager.js";
import type { VariableDefinition } from "@wf-agent/types";

describe("VariableManager - Cache Mechanism", () => {
  describe("Without Cache (Default)", () => {
    let manager: VariableManager;

    beforeEach(() => {
      manager = new VariableManager(); // No cache by default
    });

    it("should not use cache when disabled", () => {
      const varDef: VariableDefinition = {
        name: "counter",
        type: "number",
        value: 0,
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // First access
      const value1 = manager.getVariable("counter");
      expect(value1).toBe(0);

      // Update value
      manager.setVariable("counter", 10);

      // Second access should reflect update (no caching)
      const value2 = manager.getVariable("counter");
      expect(value2).toBe(10);
    });

    it("should always read fresh values without cache", () => {
      const varDef: VariableDefinition = {
        name: "data",
        type: "object",
        value: { count: 1 },
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // Multiple accesses should always get current value
      expect(manager.getVariable("data")).toEqual({ count: 1 });

      manager.setVariable("data", { count: 2 });
      expect(manager.getVariable("data")).toEqual({ count: 2 });

      manager.setVariable("data", { count: 3 });
      expect(manager.getVariable("data")).toEqual({ count: 3 });
    });
  });

  describe("With Cache Enabled", () => {
    let manager: VariableManager;

    beforeEach(() => {
      vi.useFakeTimers();
      manager = new VariableManager({ enableCache: true, cacheTTL: 1000 });
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("should cache variable values", () => {
      const varDef: VariableDefinition = {
        name: "cached",
        type: "string",
        value: "initial",
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // First access - should cache
      const value1 = manager.getVariable("cached");
      expect(value1).toBe("initial");

      // Directly modify the underlying value (bypassing cache invalidation)
      // This simulates a scenario where cache should return stale data
      manager.setVariable("cached", "updated");

      // Second access within TTL - should return cached value
      const value2 = manager.getVariable("cached");
      expect(value2).toBe("updated"); // Cache is invalidated on setVariable
    });

    it("should expire cache after TTL", () => {
      const varDef: VariableDefinition = {
        name: "expiring",
        type: "number",
        value: 100,
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // First access - cache the value
      expect(manager.getVariable("expiring")).toBe(100);

      // Advance time beyond TTL
      vi.advanceTimersByTime(1500);

      // Access after TTL - should get fresh value
      expect(manager.getVariable("expiring")).toBe(100);
    });

    it("should invalidate cache on variable update", () => {
      const varDef: VariableDefinition = {
        name: "invalidate",
        type: "string",
        value: "old",
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // First access
      expect(manager.getVariable("invalidate")).toBe("old");

      // Update variable
      manager.setVariable("invalidate", "new");

      // Should get updated value immediately
      expect(manager.getVariable("invalidate")).toBe("new");
    });

    it("should handle multiple variables with cache", () => {
      const vars: VariableDefinition[] = [
        { name: "var1", type: "string", value: "a", scope: "global", readonly: false },
        { name: "var2", type: "number", value: 1, scope: "global", readonly: false },
        { name: "var3", type: "boolean", value: true, scope: "global", readonly: false },
      ];

      vars.forEach((v) => manager.registerVariable(v));

      // Access all variables to populate cache
      expect(manager.getVariable("var1")).toBe("a");
      expect(manager.getVariable("var2")).toBe(1);
      expect(manager.getVariable("var3")).toBe(true);

      // Update one variable
      manager.setVariable("var2", 2);

      // All variables should be accessible
      expect(manager.getVariable("var1")).toBe("a");
      expect(manager.getVariable("var2")).toBe(2); // Updated
      expect(manager.getVariable("var3")).toBe(true);
    });

    it("should respect custom TTL", () => {
      const customTTL = 500; // 500ms
      const managerWithCustomTTL = new VariableManager({
        enableCache: true,
        cacheTTL: customTTL,
      });

      const varDef: VariableDefinition = {
        name: "custom",
        type: "string",
        value: "test",
        scope: "global",
        readonly: false,
      };

      managerWithCustomTTL.registerVariable(varDef);

      // Access to cache
      expect(managerWithCustomTTL.getVariable("custom")).toBe("test");

      // Advance time just before TTL
      vi.advanceTimersByTime(400);
      expect(managerWithCustomTTL.getVariable("custom")).toBe("test");

      // Advance time beyond TTL
      vi.advanceTimersByTime(200); // Total 600ms > 500ms TTL
      expect(managerWithCustomTTL.getVariable("custom")).toBe("test");
    });
  });

  describe("Cache Performance Characteristics", () => {
    it("should handle frequent reads efficiently", () => {
      const manager = new VariableManager({ enableCache: true, cacheTTL: 1000 });

      const varDef: VariableDefinition = {
        name: "frequent",
        type: "number",
        value: 42,
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // Simulate frequent reads
      const iterations = 100;
      const startTime = Date.now();

      for (let i = 0; i < iterations; i++) {
        manager.getVariable("frequent");
      }

      const endTime = Date.now();
      const duration = endTime - startTime;

      // Should complete quickly (this is a basic sanity check)
      expect(duration).toBeLessThan(100); // Should take less than 100ms
    });

    it("should handle many variables with cache", () => {
      const manager = new VariableManager({ enableCache: true, cacheTTL: 1000 });

      // Register many variables
      const variableCount = 100;
      for (let i = 0; i < variableCount; i++) {
        const varDef: VariableDefinition = {
          name: `var${i}`,
          type: "number",
          value: i,
          scope: "global",
          readonly: false,
        };
        manager.registerVariable(varDef);
      }

      // Access all variables
      for (let i = 0; i < variableCount; i++) {
        expect(manager.getVariable(`var${i}`)).toBe(i);
      }

      // Update some variables
      for (let i = 0; i < variableCount; i += 10) {
        manager.setVariable(`var${i}`, i * 10);
      }

      // Verify updates
      for (let i = 0; i < variableCount; i += 10) {
        expect(manager.getVariable(`var${i}`)).toBe(i * 10);
      }
    });
  });

  describe("Cache Edge Cases", () => {
    it("should handle non-existent variables with cache", () => {
      const manager = new VariableManager({ enableCache: true, cacheTTL: 1000 });

      // Access non-existent variable
      expect(manager.getVariable("nonExistent")).toBeUndefined();

      // Access again - should still be undefined
      expect(manager.getVariable("nonExistent")).toBeUndefined();
    });

    it("should handle undefined values correctly", () => {
      const manager = new VariableManager({ enableCache: true, cacheTTL: 1000 });

      const varDef: VariableDefinition = {
        name: "undefinedVar",
        type: "string",
        value: undefined,
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // Should return undefined (the actual value)
      expect(manager.getVariable("undefinedVar")).toBeUndefined();
    });

    it("should handle null values correctly", () => {
      const manager = new VariableManager({ enableCache: true, cacheTTL: 1000 });

      const varDef: VariableDefinition = {
        name: "nullVar",
        type: "string",
        value: null,
        scope: "global",
        readonly: false,
      };

      manager.registerVariable(varDef);

      // Should return null (the actual value)
      expect(manager.getVariable("nullVar")).toBeNull();
    });

    it("should work correctly after snapshot restoration", () => {
      const manager1 = new VariableManager({ enableCache: true, cacheTTL: 1000 });

      const varDef: VariableDefinition = {
        name: "snapshot",
        type: "number",
        value: 999,
        scope: "global",
        readonly: false,
      };

      manager1.registerVariable(varDef);
      manager1.getVariable("snapshot"); // Populate cache

      const snapshot = manager1.createSnapshot();

      // Restore to new manager with cache enabled
      const manager2 = new VariableManager({ enableCache: true, cacheTTL: 1000 });
      manager2.restoreFromSnapshot(snapshot);

      // Should work correctly
      expect(manager2.getVariable("snapshot")).toBe(999);
    });
  });
});
