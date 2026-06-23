/**
 * Python AST Analyzer Strategy — Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { PythonASTAnalyzerStrategy } from "../ast-analyzer.js";
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
// PythonASTAnalyzerStrategy
// =========================================================================

describe("PythonASTAnalyzerStrategy", () => {
  let strategy: PythonASTAnalyzerStrategy;
  let mockTerminalService: ReturnType<typeof createMockTerminalService>;

  beforeEach(() => {
    mockTerminalService = createMockTerminalService();
    const builtinHook = new PythonBuiltinHookStrategy(
      mockTerminalService as unknown as TerminalService,
    );
    strategy = new PythonASTAnalyzerStrategy(builtinHook);
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("identity", () => {
    it("should have correct id", () => {
      expect(strategy.id).toBe("ast-analyzer");
    });

    it("should have correct name", () => {
      expect(strategy.name).toContain("AST Analyzer");
    });

    it("should have correct description", () => {
      expect(strategy.description).toContain("AST");
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
      expect(result.error).toContain("Empty Python code");
    });

    it("should return error for undefined code", async () => {
      const result = await strategy.execute(
        { command: undefined } as unknown as StrategyExecuteOptions,
        defaultPolicy,
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain("Empty Python code");
    });

    it("should report executionTime", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.executionTime).toBeGreaterThanOrEqual(0);
    });

    it("should set scriptName to sandbox-python", async () => {
      const result = await strategy.execute(baseOptions, defaultPolicy);
      expect(result.scriptName).toBe("sandbox-python");
    });

    it("should handle syntax errors in code", async () => {
      const invalidCode = "def foo(\n"; // Invalid Python syntax
      const result = await strategy.execute({ command: invalidCode }, defaultPolicy);

      // AST analyzer should detect syntax error
      expect(result.success).toBe(false);
    });
  });

  describe("constructor", () => {
    it("should accept builtinHook via dependency injection", () => {
      const mockTerminal = createMockTerminalService();
      const builtinHook = new PythonBuiltinHookStrategy(mockTerminal as unknown as TerminalService);
      const strat = new PythonASTAnalyzerStrategy(builtinHook);

      expect(strat).toBeInstanceOf(PythonASTAnalyzerStrategy);
    });

    it("should create builtinHook internally when not provided", () => {
      const strat = new PythonASTAnalyzerStrategy();
      expect(strat).toBeInstanceOf(PythonASTAnalyzerStrategy);
    });
  });
});
