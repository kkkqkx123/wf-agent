/**
 * Python Builtin Hook Strategy — Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { PythonBuiltinHookStrategy } from "../builtin-hook.js";
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
  python: {
    allowedModules: [],
    deniedModules: ["os", "subprocess", "shutil"],
    allowSubprocess: false,
    restrictBuiltinOpen: true,
    allowDynamicEval: false,
  },
  resource: {
    timeoutLimit: 10000,
  },
};

// =========================================================================
// PythonBuiltinHookStrategy
// =========================================================================

describe("PythonBuiltinHookStrategy", () => {
  let strategy: PythonBuiltinHookStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    strategy = new PythonBuiltinHookStrategy(mockTerminalService as unknown as TerminalService);
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
      expect(strategy.name).toContain("Python");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("Python");
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
      expect(result.error).toContain("Empty Python code");
    });

    it("should return error for undefined code", async () => {
      const result = await strategy.execute({ command: undefined } as unknown as StrategyExecuteOptions, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Empty Python code");
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
        stderr: "python error",
        exitCode: 1,
        error: "runtime error",
      });

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.stderr).toBe("python error");
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

    it("should set scriptName to sandbox-python", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.scriptName).toBe("sandbox-python");
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

    it("should set PYTHONPATH to empty", async () => {
      await strategy.execute(baseOptions, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          env: expect.objectContaining({
            PYTHONPATH: "",
          }),
        }),
      );
    });

    it("should set PYTHONDONTWRITEBYTECODE to 1", async () => {
      await strategy.execute(baseOptions, defaultPolicy);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          env: expect.objectContaining({
            PYTHONDONTWRITEBYTECODE: "1",
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
        deniedModules: ["os", "subprocess"],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain('["os","subprocess"]');
      expect(wrapped).toContain("User code follows");
      expect(wrapped).toContain('print("hello")');
    });

    it("should generate wrapper with allowed modules", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: ["math", "json"],
        deniedModules: [],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain('["math","json"]');
    });

    it("should include safe open when restrictBuiltinOpen is true", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain("_safe_open");
    });

    it("should include VFS writable dirs when vfsEnabled is true", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy, true, ["/workspace"]);

      expect(wrapped).toContain("_VFS_ENABLED = True");
      expect(wrapped).toContain('["/workspace"]');
    });

    it("should disable subprocess when not allowed", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain('sys.modules["subprocess"] = None');
    });

    it("should disable eval, exec, compile builtins", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain('_DENIED_BUILTINS = {"eval", "exec", "compile"}');
    });

    it("should clear sys.path when no allowed modules", () => {
      const code = 'print("hello")';
      const policy = {
        allowedModules: [],
        deniedModules: [],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      };

      const wrapped = strategy.wrapWithSandbox(code, policy);

      expect(wrapped).toContain("sys.path = []");
    });
  });
});
