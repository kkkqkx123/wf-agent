import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { SandboxShellExecutor } from "../sandbox-shell-executor.js";
import type { BaseExecuteOptions } from "../base-executor.js";

// ---------------------------------------------------------------------------
// Mock sandbox-runtime module
// ---------------------------------------------------------------------------
const { mockIsEnabled, mockCreateRuntime, mockStrategyExecute, mockResetRuntime } = vi.hoisted(
  () => {
    const mockStrategyExecute = vi.fn();
    const mockCreateRuntime = vi.fn();
    const mockIsEnabled = vi.fn();
    const mockResetRuntime = vi.fn();

    return { mockIsEnabled, mockCreateRuntime, mockStrategyExecute, mockResetRuntime };
  },
);

vi.mock("../../../../services/sandbox/sandbox-runtime.js", () => ({
  getSandboxRuntime: () => ({
    isEnabled: mockIsEnabled,
    createRuntime: mockCreateRuntime,
  }),
  resetSandboxRuntime: mockResetRuntime,
}));

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe("SandboxShellExecutor", () => {
  let executor: SandboxShellExecutor;
  let mockTerminalService: any;

  beforeEach(() => {
    mockResetRuntime();
    mockStrategyExecute.mockReset();
    mockCreateRuntime.mockReset();
    mockIsEnabled.mockReset();

    mockTerminalService = {
      executeOneOff: vi.fn().mockResolvedValue({
        success: true,
        stdout: "executed",
        stderr: "",
        exitCode: 0,
      }),
    };
    executor = new SandboxShellExecutor(mockTerminalService);
  });

  afterEach(() => {
    mockResetRuntime();
  });

  const baseOptions: BaseExecuteOptions = {
    command: 'echo "hello"',
  };

  describe("execute", () => {
    it("should delegate to DirectExecutor when sandbox config is undefined", async () => {
      mockIsEnabled.mockReturnValue(false);

      const result = await executor.execute(baseOptions);

      expect(result.success).toBe(true);
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
      expect(mockCreateRuntime).not.toHaveBeenCalled();
    });

    it("should delegate to DirectExecutor when sandbox mode is disabled", async () => {
      mockIsEnabled.mockReturnValue(false);

      const options: BaseExecuteOptions = {
        ...baseOptions,
        sandboxConfig: { mode: "disabled" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(true);
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
      expect(mockCreateRuntime).not.toHaveBeenCalled();
    });

    it("should use sandbox strategy when sandbox is enabled", async () => {
      mockIsEnabled.mockReturnValue(true);
      mockStrategyExecute.mockResolvedValue({
        success: true,
        stdout: "safe command",
        stderr: "",
        exitCode: 0,
      });
      mockCreateRuntime.mockResolvedValue({
        strategy: { execute: mockStrategyExecute },
        vfs: null,
        policy: { mode: "strict" },
      });

      const options: BaseExecuteOptions = {
        ...baseOptions,
        sandboxConfig: { mode: "strict" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(true);
      expect(mockStrategyExecute).toHaveBeenCalled();
      expect(mockIsEnabled).toHaveBeenCalledWith(options.sandboxConfig);
    });

    it("should block dangerous commands in strict mode", async () => {
      mockIsEnabled.mockReturnValue(true);
      mockStrategyExecute.mockResolvedValue({
        success: false,
        scriptName: "sandbox-shell",
        error: "Sandbox denied: dangerous command",
        executionTime: 0,
      });
      mockCreateRuntime.mockResolvedValue({
        strategy: { execute: mockStrategyExecute },
        vfs: null,
        policy: { mode: "strict" },
      });

      const options: BaseExecuteOptions = {
        command: "sudo rm -rf /",
        sandboxConfig: { mode: "strict" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it("should allow dangerous commands in lenient mode with warning", async () => {
      mockIsEnabled.mockReturnValue(true);
      mockStrategyExecute.mockResolvedValue({
        success: false,
        scriptName: "sandbox-shell",
        error: "Sandbox denied: dangerous command",
        executionTime: 0,
      });

      // Simulate the lenient wrapper: when the strategy returns denied,
      // the wrapper converts to success + stderr warning
      mockCreateRuntime.mockImplementation(async (_language, _options, _config) => {
        const innerStrategy = { execute: mockStrategyExecute };
        const lenientWrapper = {
          execute: async (execOptions: any, policy: any) => {
            const result = await innerStrategy.execute(execOptions, policy);
            if (!result.success && result.error?.startsWith("Sandbox denied")) {
              return {
                success: true,
                scriptName: result.scriptName,
                stdout: result.stderr ?? "",
                stderr: `[sandbox-lenient-warning] ${result.error}`,
                exitCode: 0,
                executionTime: result.executionTime,
              };
            }
            return result;
          },
        };
        return {
          strategy: lenientWrapper,
          vfs: null,
          policy: { mode: "lenient" },
        };
      });

      const options: BaseExecuteOptions = {
        command: "sudo rm -rf /",
        sandboxConfig: { mode: "lenient" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(true);
      expect(result.stderr).toContain("sandbox-lenient-warning");
    });

    it("should apply custom shell policy", async () => {
      mockIsEnabled.mockReturnValue(true);
      mockStrategyExecute.mockResolvedValue({
        success: false,
        scriptName: "sandbox-shell",
        error: "Sandbox denied: wget is denied by policy",
        executionTime: 0,
      });
      mockCreateRuntime.mockResolvedValue({
        strategy: { execute: mockStrategyExecute },
        vfs: null,
        policy: { mode: "strict" },
      });

      const options: BaseExecuteOptions = {
        command: "wget http://example.com",
        sandboxConfig: {
          mode: "strict",
          policy: {
            shell: {
              deniedCommands: ["wget"],
            },
          },
        },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(false);
      expect(result.error).toContain("denied");
      expect(mockCreateRuntime).toHaveBeenCalledWith(
        "shell",
        expect.any(Object),
        options.sandboxConfig,
      );
    });

    it("should set sandbox-shell profile in scriptName when profile is specified", async () => {
      mockIsEnabled.mockReturnValue(true);
      mockStrategyExecute.mockResolvedValue({
        success: true,
        scriptName: "sandbox-shell:secure-dev",
        stdout: "result",
      });
      mockCreateRuntime.mockResolvedValue({
        strategy: { execute: mockStrategyExecute },
        vfs: null,
        policy: { mode: "strict" },
      });

      const options: BaseExecuteOptions = {
        ...baseOptions,
        sandboxConfig: {
          mode: "strict",
          profile: "secure-dev",
        },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(true);
      expect(result.scriptName).toContain("sandbox-shell");
    });

    it("should handle errors from strategy execution gracefully", async () => {
      mockIsEnabled.mockReturnValue(true);
      mockStrategyExecute.mockRejectedValue(new Error("Strategy failure"));
      mockCreateRuntime.mockResolvedValue({
        strategy: { execute: mockStrategyExecute },
        vfs: null,
        policy: { mode: "strict" },
      });

      const options: BaseExecuteOptions = {
        command: "error-command",
        sandboxConfig: { mode: "strict" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(result.scriptName).toBe("sandbox-shell");
    });
  });

  describe("cleanup", () => {
    it("should cleanup without errors", async () => {
      await expect(executor.cleanup()).resolves.toBeUndefined();
    });
  });
});
