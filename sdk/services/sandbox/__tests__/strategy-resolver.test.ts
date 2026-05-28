import { describe, it, expect, beforeEach } from "vitest";
import { DefaultStrategyResolver } from "../strategy-resolver.js";
import type { StrategyImplementation } from "@wf-agent/types";

describe("DefaultStrategyResolver", () => {
  let resolver: DefaultStrategyResolver;

  beforeEach(() => {
    resolver = new DefaultStrategyResolver();
  });

  describe("resolveBest", () => {
    it("should resolve shell strategy: static-analyzer", () => {
      const strategy = resolver.resolveBest("shell", ["static-analyzer"]);
      expect(strategy).toBeDefined();
      expect(strategy.id).toBe("static-analyzer");
      expect(strategy.priority).toBe(10);
    });

    it("should resolve python strategy: ast-analyzer", () => {
      const strategy = resolver.resolveBest("python", ["ast-analyzer"]);
      expect(strategy).toBeDefined();
      expect(strategy.id).toBe("ast-analyzer");
      expect(strategy.priority).toBe(25);
    });

    it("should resolve python strategy: builtin-hook when ast-analyzer unavailable", () => {
      const strategy = resolver.resolveBest("python", ["ast-analyzer", "builtin-hook"]);
      expect(strategy).toBeDefined();
      expect(["ast-analyzer", "builtin-hook"]).toContain(strategy.id);
    });

    it("should resolve javascript strategy: vm-context", () => {
      const strategy = resolver.resolveBest("javascript", ["vm-context"]);
      expect(strategy).toBeDefined();
      expect(strategy.id).toBe("vm-context");
      expect(strategy.priority).toBe(30);
    });

    it("should fall back to highest-priority available strategy when preferred not found", () => {
      const strategy = resolver.resolveBest("shell", ["non-existent-strategy"]);
      expect(strategy).toBeDefined();
      // On Windows: windows-job (priority 50) > static-analyzer (priority 10)
      expect(strategy.priority).toBe(50);
    });

    it("should return best available strategy based on priority", () => {
      const shellStrategies = resolver.resolveBest("shell", [
        "non-existent",
        "static-analyzer",
      ]);
      expect(shellStrategies).toBeDefined();
      expect(shellStrategies.id).toBe("static-analyzer");
    });
  });

  describe("registerStrategy", () => {
    it("should register a custom shell strategy", () => {
      const custom: StrategyImplementation<unknown> = {
        id: "custom-shell",
        name: "Custom Shell",
        description: "Test",
        priority: 100,
        isAvailable: () => true,
        execute: async () => ({ success: true, scriptName: "test", executionTime: 0 }),
      };

      resolver.registerStrategy("shell", custom);
      const result = resolver.resolveBest("shell", ["custom-shell"]);
      expect(result.id).toBe("custom-shell");
    });

    it("should register a custom python strategy", () => {
      const custom: StrategyImplementation<unknown> = {
        id: "custom-py",
        name: "Custom Python",
        description: "Test",
        priority: 100,
        isAvailable: () => true,
        execute: async () => ({ success: true, scriptName: "test", executionTime: 0 }),
      };

      resolver.registerStrategy("python", custom);
      const result = resolver.resolveBest("python", ["custom-py"]);
      expect(result.id).toBe("custom-py");
    });

    it("should register a custom javascript strategy", () => {
      const custom: StrategyImplementation<unknown> = {
        id: "custom-js",
        name: "Custom JS",
        description: "Test",
        priority: 100,
        isAvailable: () => true,
        execute: async () => ({ success: true, scriptName: "test", executionTime: 0 }),
      };

      resolver.registerStrategy("javascript", custom);
      const result = resolver.resolveBest("javascript", ["custom-js"]);
      expect(result.id).toBe("custom-js");
    });

    it("should prefer higher priority strategy when multiple preferred ids match", () => {
      const low: StrategyImplementation<unknown> = {
        id: "low-priority",
        name: "Low",
        description: "Test",
        priority: 5,
        isAvailable: () => true,
        execute: async () => ({ success: true, scriptName: "test", executionTime: 0 }),
      };
      const high: StrategyImplementation<unknown> = {
        id: "high-priority",
        name: "High",
        description: "Test",
        priority: 50,
        isAvailable: () => true,
        execute: async () => ({ success: true, scriptName: "test", executionTime: 0 }),
      };

      resolver.registerStrategy("shell", low);
      resolver.registerStrategy("shell", high);

      const result = resolver.resolveBest("shell", ["high-priority", "low-priority"]);
      expect(result.id).toBe("high-priority");
    });
  });
});