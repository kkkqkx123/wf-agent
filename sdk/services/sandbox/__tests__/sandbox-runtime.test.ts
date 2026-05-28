import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  SandboxRuntime,
  resetSandboxRuntime,
  getSandboxRuntime,
} from "../sandbox-runtime.js";
import type { SandboxConfig, StrategyExecuteOptions } from "@wf-agent/types";

describe("SandboxRuntime", () => {
  let runtime: SandboxRuntime;

  beforeEach(() => {
    resetSandboxRuntime();
    runtime = new SandboxRuntime();
  });

  afterEach(() => {
    resetSandboxRuntime();
  });

  describe("isEnabled", () => {
    it("should return false when config is undefined", () => {
      expect(runtime.isEnabled(undefined)).toBe(false);
    });

    it("should return false when config.mode is disabled", () => {
      expect(runtime.isEnabled({ mode: "disabled" })).toBe(false);
    });

    it("should return true when config.mode is strict", () => {
      expect(runtime.isEnabled({ mode: "strict" })).toBe(true);
    });

    it("should return true when config.mode is lenient", () => {
      expect(runtime.isEnabled({ mode: "lenient" })).toBe(true);
    });

    it("should return true when config.mode is custom", () => {
      expect(runtime.isEnabled({ mode: "custom" })).toBe(true);
    });
  });

  describe("createRuntime", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: "echo test",
    };

    it("should return null strategy and vfs when sandbox is disabled", async () => {
      const config: SandboxConfig = { mode: "disabled" };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.strategy).toBeNull();
      expect(result.vfs).toBeNull();
      expect(result.policy.mode).toBe("strict");
    });

    it("should return null strategy and vfs when config is undefined", async () => {
      const result = await runtime.createRuntime("shell", baseOptions, undefined);

      expect(result.strategy).toBeNull();
      expect(result.vfs).toBeNull();
    });

    it("should return a valid strategy for shell language", async () => {
      const config: SandboxConfig = { mode: "strict" };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.strategy).not.toBeNull();
      // Default shellStrategy = ["os-hook", "static-analyzer"];
      // On Windows, "os-hook" resolves to "windows-job" (priority 50) which is preferred over "static-analyzer"
      expect(result.strategy!.priority).toBeGreaterThanOrEqual(10);
    });

    it("should return a valid strategy for python language", async () => {
      const config: SandboxConfig = { mode: "strict" };
      const result = await runtime.createRuntime("python", baseOptions, config);

      expect(result.strategy).not.toBeNull();
    });

    it("should return a valid strategy for javascript language", async () => {
      const config: SandboxConfig = { mode: "strict" };
      const result = await runtime.createRuntime("javascript", baseOptions, config);

      expect(result.strategy).not.toBeNull();
      expect(result.strategy!.id).toBe("vm-context");
    });

    it("should merge policy from config", async () => {
      const config: SandboxConfig = {
        mode: "strict",
        policy: {
          shell: {
            allowedCommands: ["echo"],
          },
        },
      };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.policy.shell?.allowedCommands).toContain("echo");
      expect(result.policy.mode).toBe("strict");
    });

    it("should respect shellStrategy preference", async () => {
      const config: SandboxConfig = {
        mode: "strict",
        shellStrategy: ["static-analyzer"],
      };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.strategy).not.toBeNull();
      expect(result.strategy!.id).toBe("static-analyzer");
    });

    it("should wrap strategy in lenient mode wrapper", async () => {
      const config: SandboxConfig = { mode: "lenient" };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.strategy).not.toBeNull();
      expect(result.strategy!.description).toContain("lenient");
    });

    it("should create VFS when vfs config is enabled", async () => {
      const config: SandboxConfig = {
        mode: "strict",
        vfs: {
          enabled: true,
          workspaceRoot: "/test/workspace",
        },
      };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.vfs).not.toBeNull();
    });

    it("should not create VFS when vfs config is not provided", async () => {
      const config: SandboxConfig = { mode: "strict" };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.vfs).toBeNull();
    });

    it("should return default policy with strict mode by default", async () => {
      const config: SandboxConfig = { mode: "strict" };
      const result = await runtime.createRuntime("shell", baseOptions, config);

      expect(result.policy.mode).toBe("strict");
      expect(result.policy.shell).toBeDefined();
      expect(result.policy.filesystem).toBeDefined();
      expect(result.policy.process).toBeDefined();
      expect(result.policy.network).toBeDefined();
      expect(result.policy.resource).toBeDefined();
    });
  });

  describe("getSandboxRuntime (singleton)", () => {
    it("should return the same instance", () => {
      resetSandboxRuntime();
      const instance1 = getSandboxRuntime();
      const instance2 = getSandboxRuntime();
      expect(instance1).toBe(instance2);
    });

    it("should create a new instance after reset", () => {
      resetSandboxRuntime();
      const instance1 = getSandboxRuntime();
      resetSandboxRuntime();
      const instance2 = getSandboxRuntime();
      expect(instance1).not.toBe(instance2);
    });
  });

  describe("registerStrategy", () => {
    it("should register a custom strategy and resolve it", async () => {
      const custom = {
        id: "custom-strategy",
        name: "Custom",
        description: "Custom strategy",
        priority: 99,
        isAvailable: () => true,
        execute: async () => ({
          success: true,
          scriptName: "custom",
          executionTime: 0,
        }),
      };

      runtime.registerStrategy("shell", custom);

      const config: SandboxConfig = {
        mode: "strict",
        shellStrategy: ["custom-strategy"],
      };

      const result = await runtime.createRuntime("shell", { command: "test" }, config);
      expect(result.strategy).not.toBeNull();
      expect(result.strategy!.id).toBe("custom-strategy");
    });
  });

  describe("legacy mappings", () => {
    it("should apply docker legacy mapping", async () => {
      const config: SandboxConfig = {
        mode: "strict",
        type: "docker",
      };
      const result = await runtime.createRuntime("shell", { command: "test" }, config);

      expect(result.strategy).not.toBeNull();
      // Docker maps to shellStrategy: ["container"]; no container strategy exists,
      // so it falls back to highest-priority available strategy (windows-job on Windows)
      expect(result.strategy!.priority).toBeGreaterThanOrEqual(10);
    });

    it("should apply python legacy mapping", async () => {
      const config: SandboxConfig = {
        mode: "strict",
        type: "python",
      };
      const result = await runtime.createRuntime("python", { command: "print('hello')" }, config);

      expect(result.strategy).not.toBeNull();
    });
  });
});