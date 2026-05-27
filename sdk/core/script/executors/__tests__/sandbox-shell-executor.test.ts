import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { SandboxShellExecutor } from "../sandbox-shell-executor.js";
import { resetSandboxRuntime } from "../../../services/sandbox/sandbox-runtime.js";
import type { BaseExecuteOptions } from "./base-executor.js";
import type { SandboxConfig } from "@wf-agent/types";

describe("SandboxShellExecutor", () => {
  let executor: SandboxShellExecutor;
  let mockTerminalService: any;

  beforeEach(() => {
    resetSandboxRuntime();
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
    resetSandboxRuntime();
  });

  const baseOptions: BaseExecuteOptions = {
    command: 'echo "hello"',
  };

  describe("execute", () => {
    it("should delegate to DirectExecutor when sandbox config is undefined", async () => {
      const result = await executor.execute(baseOptions);

      expect(result.success).toBe(true);
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
    });

    it("should delegate to DirectExecutor when sandbox mode is disabled", async () => {
      const options: BaseExecuteOptions = {
        ...baseOptions,
        sandboxConfig: { mode: "disabled" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(true);
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
    });

    it("should use sandbox strategy when sandbox is enabled", async () => {
      mockTerminalService.executeOneOff.mockResolvedValue({
        success: true,
        stdout: "safe command",
        stderr: "",
        exitCode: 0,
      });

      const options: BaseExecuteOptions = {
        ...baseOptions,
        sandboxConfig: { mode: "strict" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(true);
      expect(mockTerminalService.executeOneOff).toHaveBeenCalled();
    });

    it("should block dangerous commands in strict mode", async () => {
      const options: BaseExecuteOptions = {
        command: "sudo rm -rf /",
        sandboxConfig: { mode: "strict" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it("should allow dangerous commands in lenient mode with warning", async () => {
      const options: BaseExecuteOptions = {
        command: "sudo rm -rf /",
        sandboxConfig: { mode: "lenient" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(true);
      expect(result.stderr).toContain("sandbox-lenient-warning");
    });

    it("should apply custom shell policy", async () => {
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
    });

    it("should set sandbox-shell profile in scriptName when profile is specified", async () => {
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
      mockTerminalService.executeOneOff.mockRejectedValue(new Error("Terminal failure"));

      const options: BaseExecuteOptions = {
        command: "error-command",
        sandboxConfig: { mode: "strict" },
      };

      const result = await executor.execute(options);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });
  });

  describe("cleanup", () => {
    it("should cleanup without errors", async () => {
      await expect(executor.cleanup()).resolves.toBeUndefined();
    });
  });
});