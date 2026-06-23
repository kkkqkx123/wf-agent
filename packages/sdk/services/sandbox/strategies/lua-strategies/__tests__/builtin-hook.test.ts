/**
 * Lua Builtin Hook Strategy — Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { LuaBuiltinHookStrategy } from "../builtin-hook.js";
import type { SandboxPolicy, StrategyExecuteOptions } from "@wf-agent/types";
import type { TerminalService } from "../../../../terminal/index.js";

// =========================================================================
// Test Helpers
// =========================================================================

function createMockTerminalService() {
  return {
    executeOneOff: vi.fn().mockResolvedValue({
      success: true,
      stdout: "executed",
      stderr: "",
      exitCode: 0,
    }),
    spawnProcess: vi.fn(),
    monitorProcess: vi.fn(),
  };
}

const defaultPolicy: SandboxPolicy = {
  mode: "strict",
  lua: {
    allowedModules: [],
    deniedModules: ["os", "io", "package", "debug"],
    allowOsExecute: false,
    restrictIoOpen: true,
    allowDynamicLoad: false,
  },
  resource: {
    timeoutLimit: 10000,
  },
};

// =========================================================================
// LuaBuiltinHookStrategy
// =========================================================================

describe("LuaBuiltinHookStrategy", () => {
  let strategy: LuaBuiltinHookStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new LuaBuiltinHookStrategy(mockTerminalService as unknown as TerminalService);
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("builtin-hook");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("Lua");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("Lua");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(20);
    });
  });

  describe("isAvailable", () => {
    it("should return a boolean", () => {
      const result = strategy.isAvailable();
      expect(typeof result).toBe("boolean");
    });
  });

  describe("execute", () => {
    const baseOptions: StrategyExecuteOptions = {
      command: 'print("hello")',
    };

    it("should return error for empty code", async () => {
      const result = await strategy.execute({ command: "" }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Empty Lua code");
    });

    it("should return error for undefined code", async () => {
      const result = await strategy.execute(
        { command: undefined } as unknown as StrategyExecuteOptions,
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("Empty Lua code");
    });

    it("should execute code via terminalService", async () => {
      mockTerminalService.executeOneOff.mockResolvedValue({
        success: true,
        stdout: "hello",
        stderr: "",
        exitCode: 0,
      });

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
      expect(result.success).toBe(true);
      expect(result.stdout).toBe("hello");
    });

    it("should handle terminalService failure", async () => {
      mockTerminalService.executeOneOff.mockResolvedValue({
        success: false,
        stdout: "",
        stderr: "lua error",
        exitCode: 1,
        error: "runtime error",
      });

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.stderr).toBe("lua error");
      expect(result.error).toBe("runtime error");
    });

    it("should handle terminalService exception", async () => {
      mockTerminalService.executeOneOff.mockRejectedValue(new Error("spawn failed"));

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toBe("spawn failed");
    });

    it("should handle non-Error exception", async () => {
      mockTerminalService.executeOneOff.mockRejectedValue("string error");

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toBe("string error");
    });

    it("should report executionTime", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
      expect(typeof result.executionTime).toBe("number");
    });

    it("should set scriptName to sandbox-lua", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.scriptName).toBe("sandbox-lua");
    });

    it("should pass timeout from options", async () => {
      const opts: StrategyExecuteOptions = {
        command: 'print("test")',
        timeout: 5000,
      };

      await strategy.execute(opts, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          timeout: 5000,
        }),
      );
    });

    it("should pass timeout from policy when options.timeout is undefined", async () => {
      const opts: StrategyExecuteOptions = {
        command: 'print("test")',
      };

      await strategy.execute(opts, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          timeout: 10000,
        }),
      );
    });

    it("should pass cwd and env to terminalService", async () => {
      const opts: StrategyExecuteOptions = {
        command: 'print("test")',
        cwd: "/workspace",
        env: { FOO: "bar" },
      };

      await strategy.execute(opts, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          cwd: "/workspace",
          env: expect.objectContaining({ FOO: "bar" }),
        }),
      );
    });

    it("should set LUA_PATH and LUA_CPATH to empty", async () => {
      await strategy.execute(baseOptions, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          env: expect.objectContaining({
            LUA_PATH: "",
            LUA_CPATH: "",
          }),
        }),
      );
    });
  });

  describe("wrapWithSandbox", () => {
    it("should generate wrapper with denied modules", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: ["os", "io"],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain('["os"] = true');
      expect(wrapped).toContain('["io"] = true');
      expect(wrapped).toContain("User code follows");
      expect(wrapped).toContain('print("hello")');
    });

    it("should generate wrapper with allowed modules", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: ["math", "string"],
        deniedModules: [],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain('["math"] = true');
      expect(wrapped).toContain('["string"] = true');
    });

    it("should disable os.execute when not allowed", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain("os.execute = nil");
    });

    it("should not disable os.execute when allowed", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowOsExecute: true,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      // When allowOsExecute is true, the if block should not execute
      expect(wrapped).toContain("if not _ALLOW_OS_EXECUTE then");
    });

    it("should disable loadstring/load when dynamic load not allowed", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain("loadstring = nil");
      expect(wrapped).toContain("load = nil");
    });

    it("should include safe io.open when restrictIoOpen is true", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain("function io.open(path, mode)");
    });

    it("should include VFS writable dirs when vfsEnabled is true", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy, true, ["/workspace"]);

      expect(wrapped).toContain("_VFS_ENABLED = true");
      expect(wrapped).toContain('"/workspace"');
    });

    it("should disable io.popen and io.tmpfile", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain("io.popen = nil");
      expect(wrapped).toContain("io.tmpfile = nil");
    });

    it("should disable os.remove, os.rename, os.exit, os.setlocale, os.tmpname", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowOsExecute: false,
        restrictIoOpen: true,
        allowDynamicLoad: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain("os.remove = nil");
      expect(wrapped).toContain("os.rename = nil");
      expect(wrapped).toContain("os.exit = nil");
      expect(wrapped).toContain("os.setlocale = nil");
      expect(wrapped).toContain("os.tmpname = nil");
    });
  });
});
