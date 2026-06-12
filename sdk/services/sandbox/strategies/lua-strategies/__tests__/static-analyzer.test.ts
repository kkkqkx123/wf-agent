/**
 * Lua Static Analyzer Strategy — Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { LuaStaticAnalyzerStrategy } from "../static-analyzer.js";
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
// LuaStaticAnalyzerStrategy
// =========================================================================

describe("LuaStaticAnalyzerStrategy", () => {
  let strategy: LuaStaticAnalyzerStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    const builtinHook = new LuaBuiltinHookStrategy(
      mockTerminalService as unknown as TerminalService,
    );
    strategy = new LuaStaticAnalyzerStrategy(builtinHook);
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("static-analyzer");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("Static Analyzer");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("Static analysis");
    });

    it("should have correct priority", () => {
      expect(strategy.priority).toBe(25);
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

    it("should pass safe code to builtin hook", async () => {
      mockTerminalService.executeOneOff.mockResolvedValue({
        success: true,
        stdout: "hello",
        stderr: "",
        exitCode: 0,
      });

      const result = await strategy.execute(baseOptions, defaultPolicy);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe("hello");
    });

    it("should detect dangerous os.execute call", async () => {
      const dangerousCode = 'os.execute("rm -rf /")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Security violation");
      expect(result.error).toContain("os.execute");
    });

    it("should allow os.execute when allowOsExecute is true", async () => {
      const policyWithOsExecute: SandboxPolicy = {
        ...defaultPolicy,
        lua: {
          ...defaultPolicy.lua!,
          allowOsExecute: true,
        },
      };

      mockTerminalService.executeOneOff.mockResolvedValue({
        success: true,
        stdout: "",
        stderr: "",
        exitCode: 0,
      });

      const code = 'os.execute("echo hello")';
      await strategy.execute({ command: code }, policyWithOsExecute);

      // Should pass static analysis and delegate to builtin hook
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
    });

    it("should detect dangerous os.remove call", async () => {
      const dangerousCode = 'os.remove("/etc/passwd")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Security violation");
      expect(result.error).toContain("os.remove");
    });

    it("should detect dangerous os.rename call", async () => {
      const dangerousCode = 'os.rename("/a", "/b")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("os.rename");
    });

    it("should detect dangerous io.popen call", async () => {
      const dangerousCode = 'io.popen("ls")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("io.popen");
    });

    it("should detect dangerous loadstring call", async () => {
      const dangerousCode = 'loadstring("print(1)")()';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("loadstring");
    });

    it("should allow loadstring when allowDynamicLoad is true", async () => {
      const policyWithDynamicLoad: SandboxPolicy = {
        ...defaultPolicy,
        lua: {
          ...defaultPolicy.lua!,
          allowDynamicLoad: true,
        },
      };

      mockTerminalService.executeOneOff.mockResolvedValue({
        success: true,
        stdout: "",
        stderr: "",
        exitCode: 0,
      });

      const code = 'loadstring("print(1)")()';
      await strategy.execute({ command: code }, policyWithDynamicLoad);

      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
    });

    it("should detect dangerous dofile call", async () => {
      const dangerousCode = 'dofile("evil.lua")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("dofile");
    });

    it("should detect dangerous loadfile call", async () => {
      const dangerousCode = 'loadfile("evil.lua")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("loadfile");
    });

    it("should detect denied module require", async () => {
      const dangerousCode = 'require("os")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Module denied");
    });

    it("should detect module not in allowed list", async () => {
      const policyWithAllowed: SandboxPolicy = {
        ...defaultPolicy,
        lua: {
          ...defaultPolicy.lua!,
          allowedModules: ["math", "string"],
          deniedModules: [],
        },
      };

      const code = 'require("os")';
      const result = await strategy.execute({ command: code }, policyWithAllowed);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Module not allowed");
    });

    it("should detect io.open with write mode", async () => {
      const dangerousCode = 'io.open("/etc/passwd", "w")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Write mode");
    });

    it("should detect io.open with append mode", async () => {
      const dangerousCode = 'io.open("/tmp/test", "a")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("Write mode");
    });

    it("should detect _G access", async () => {
      const dangerousCode = '_G["os"].execute("ls")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("_G");
    });

    it("should detect setfenv call", async () => {
      const dangerousCode = "setfenv(1, {})";
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("setfenv");
    });

    it("should detect getfenv call", async () => {
      const dangerousCode = "getfenv(1)";
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.success).toBe(false);
      expect(result.error).toContain("getfenv");
    });

    it("should report executionTime", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });

    it("should set scriptName to sandbox-lua", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.scriptName).toBe("sandbox-lua");
    });

    it("should include violations in stderr", async () => {
      const dangerousCode = 'os.execute("ls")';
      const result = await strategy.execute({ command: dangerousCode }, defaultPolicy);

      expect(result.stderr).toContain("Static analysis violations");
      expect(result.stderr).toContain("os.execute");
    });
  });

  describe("constructor", () => {
    it("should accept builtinHook via dependency injection", () => {
      const mockTerminal = createMockTerminalService();
      const builtinHook = new LuaBuiltinHookStrategy(mockTerminal as unknown as TerminalService);
      const strat = new LuaStaticAnalyzerStrategy(builtinHook);

      expect(strat).toBeInstanceOf(LuaStaticAnalyzerStrategy);
    });

    it("should create builtinHook internally when not provided", () => {
      const strat = new LuaStaticAnalyzerStrategy();
      expect(strat).toBeInstanceOf(LuaStaticAnalyzerStrategy);
    });
  });
});
