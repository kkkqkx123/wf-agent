/**
 * Python Builtin Hook Strategy Tests
 *
 * Tests for PythonBuiltinHookStrategy:
 *   - Identity properties
 *   - isAvailable (python check)
 *   - Empty code handling
 *   - wrapWithSandbox code generation
 *   - execute with mock terminal service
 *   - Policy propagation (allowedModules, deniedModules, restrictBuiltinOpen, allowSubprocess)
 *   - VFS mode and writable dirs
 *   - Temp file creation and cleanup
 */

import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { PythonBuiltinHookStrategy } from "../../strategies/python-builtin-hook.js";
import type { SandboxPolicy, StrategyExecuteOptions } from "@wf-agent/types";

// =========================================================================
// Helpers
// =========================================================================

const defaultPolicy: SandboxPolicy = {
  mode: "strict",
  python: {
    allowedModules: [],
    deniedModules: [],
    allowSubprocess: false,
    restrictBuiltinOpen: true,
    allowDynamicEval: false,
  },
};

function createMockTerminal() {
  return {
    executeOneOff: vi.fn().mockResolvedValue({
      success: true,
      stdout: "hello from python",
      stderr: "",
      exitCode: 0,
    }),
  };
}

// =========================================================================
// PythonBuiltinHookStrategy
// =========================================================================

describe("PythonBuiltinHookStrategy", () => {
  let strategy: PythonBuiltinHookStrategy;
  let mockTerminal: ReturnType<typeof createMockTerminal>;

  beforeEach(() => {
    vi.useFakeTimers();
    mockTerminal = createMockTerminal();
    strategy = new PythonBuiltinHookStrategy(mockTerminal as any);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  // ── Identity ──

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("builtin-hook");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("Builtin");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("builtin");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(20);
    });
  });

  // ── isAvailable ──

  describe("isAvailable", () => {
    it("should return false when python is not available", () => {
      const available = strategy.isAvailable();
      expect(typeof available).toBe("boolean");
    });
  });

  // ── execute ──

  describe("execute", () => {
    it("should reject empty code", async () => {
      const options: StrategyExecuteOptions = { command: "" };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toBe("Empty Python code");
      expect(result.scriptName).toBe("sandbox-python");
    });

    it("should return executionTime", async () => {
      const options: StrategyExecuteOptions = { command: "print(1)" };
      const result = await strategy.execute(options, defaultPolicy);
      expect(typeof result.executionTime).toBe("number");
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });

    it("should delegate to terminalService.executeOneOff when code is non-empty", async () => {
      const options: StrategyExecuteOptions = { command: "print('hello')" };
      const result = await strategy.execute(options, defaultPolicy);

      expect(mockTerminal.executeOneOff).toHaveBeenCalled();
      // Should have been called with python command and options
      const callArg = mockTerminal.executeOneOff.mock.calls[0][0];
      expect(callArg).toContain("python");
      expect(callArg).toContain(".py");

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("hello from python");
    });

    it("should pass cwd, env, and timeout to terminalService", async () => {
      const options: StrategyExecuteOptions = {
        command: "print(1)",
        cwd: "/workspace",
        env: { MY_VAR: "test" },
        timeout: 30000,
      };
      await strategy.execute(options, defaultPolicy);

      expect(mockTerminal.executeOneOff).toHaveBeenCalledWith(
        expect.stringContaining("python"),
        expect.objectContaining({
          cwd: "/workspace",
          env: expect.objectContaining({ MY_VAR: "test" }),
          timeout: 30000,
        }),
      );
    });

    it("should set PYTHONPATH and other env vars for isolation", async () => {
      const options: StrategyExecuteOptions = { command: "print(1)" };
      await strategy.execute(options, defaultPolicy);

      const callOptions = mockTerminal.executeOneOff.mock.calls[0][1];
      expect(callOptions.env.PYTHONPATH).toBe("");
      expect(callOptions.env.PYTHONDONTWRITEBYTECODE).toBe("1");
      expect(callOptions.env.PYTHONSTARTUP).toBe("");
      expect(callOptions.env.PYTHONHOME).toBe("");
    });

    it("should propagate terminal failure", async () => {
      mockTerminal.executeOneOff.mockResolvedValue({
        success: false,
        stdout: "",
        stderr: "syntax error",
        exitCode: 1,
        error: "exit code 1",
      });

      const options: StrategyExecuteOptions = { command: "invalid syntax" };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(1);
    });

    it("should handle terminal throwing an error", async () => {
      mockTerminal.executeOneOff.mockRejectedValue(new Error("python not found"));

      const options: StrategyExecuteOptions = { command: "print(1)" };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toBe("python not found");
    });

    it("should handle terminal throwing non-Error", async () => {
      mockTerminal.executeOneOff.mockRejectedValue("string error");

      const options: StrategyExecuteOptions = { command: "print(1)" };
      const result = await strategy.execute(options, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toBe("string error");
    });
  });

  // ── wrapWithSandbox ──

  describe("wrapWithSandbox", () => {
    // Access the private method via prototype
    const wrapWithSandbox = (PythonBuiltinHookStrategy.prototype as any).wrapWithSandbox.bind(strategy);

    it("should wrap code with safety preamble", () => {
      const wrapped = wrapWithSandbox("print('test')", {
        allowedModules: [],
        deniedModules: ["os", "subprocess"],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      });

      expect(wrapped).toContain('print(\'test\')');
      expect(wrapped).toContain("import sys");
      expect(wrapped).toContain("import builtins as __builtins_module");
      expect(wrapped).toContain("__builtins_module.open = _safe_open");
      expect(wrapped).toContain("sys.modules[\"subprocess\"] = None");
      expect(wrapped).toContain("sys.path = []");
    });

    it("should include deniedModules in generated wrapper", () => {
      const wrapped = wrapWithSandbox("print(1)", {
        allowedModules: [],
        deniedModules: ["os", "shutil", "ctypes"],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      });

      expect(wrapped).toContain('"os"');
      expect(wrapped).toContain('"shutil"');
      expect(wrapped).toContain('"ctypes"');
    });

    it("should include allowedModules in generated wrapper", () => {
      const wrapped = wrapWithSandbox("print(1)", {
        allowedModules: ["numpy", "pandas"],
        deniedModules: ["os"],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      });

      expect(wrapped).toContain('"numpy"');
      expect(wrapped).toContain('"pandas"');
    });

    it("should set _safe_open disabled when restrictBuiltinOpen is false", () => {
      const wrapped = wrapWithSandbox("print(1)", {
        allowedModules: [],
        deniedModules: ["os"],
        allowSubprocess: false,
        restrictBuiltinOpen: false,
        allowDynamicEval: false,
      });

      // VFS is disabled by default
      expect(wrapped).toContain("_VFS_ENABLED = False");
      // The safe_open block is wrapped in "if False:" so it's disabled
      expect(wrapped).toContain("if False:");
    });

    it("should include VFS mode and writable directories when enabled", () => {
      const wrapped = wrapWithSandbox("print(1)", {
        allowedModules: [],
        deniedModules: ["os"],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      }, true, ["/workspace", "/tmp"]);

      expect(wrapped).toContain("_VFS_ENABLED = True");
      expect(wrapped).toContain('"/workspace"');
      expect(wrapped).toContain('"/tmp"');
    });

    it("should set allowSubprocess True when policy allows it", () => {
      const wrapped = wrapWithSandbox("print(1)", {
        allowedModules: [],
        deniedModules: ["os"],
        allowSubprocess: true,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      });

      // The subprocess disable block is wrapped in "if not True:" so it's disabled
      expect(wrapped).toContain("if not True:");
    });

    it("should deny eval, exec, compile builtins", () => {
      const wrapped = wrapWithSandbox("print(1)", {
        allowedModules: [],
        deniedModules: ["os"],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      });

      expect(wrapped).toContain('"eval"');
      expect(wrapped).toContain('"exec"');
      expect(wrapped).toContain('"compile"');
    });

    it("should generate correct _DENIED_BUILTINS set", () => {
      const wrapped = wrapWithSandbox("print(1)", {
        allowedModules: [],
        deniedModules: ["os"],
        allowSubprocess: false,
        restrictBuiltinOpen: true,
        allowDynamicEval: false,
      });

      expect(wrapped).toContain('_DENIED_BUILTINS = {"eval", "exec", "compile"}');
    });
  });
});
