/**
 * Run-shell tool handler using Terminal Service
 *
 * Provides stateless shell command execution with:
 * - Multi-shell support
 * - Working directory management
 * - Environment variable maintenance
 * - Timeout control
 */

import type { ShellType } from "@wf-agent/sdk/services";
import { getTerminalService } from "@wf-agent/sdk/services";
import { TimeoutController } from "@wf-agent/tool-executors";
import type { ToolOutput } from "@wf-agent/types";
import type { RunShellConfig } from "../../../types.js";

/**
 * Create a run_shell tool to execute functions
 */
export function createRunShellHandler(config?: RunShellConfig) {
  const maxTimeout = config?.maxTimeout ?? 600000;
  const terminalService = getTerminalService();

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    const {
      command,
      timeout = 120,
      shell_type,
      cwd,
      env,
    } = params as {
      command: string;
      timeout?: number;
      shell_type?: ShellType;
      cwd?: string;
      env?: Record<string, string>;
    };

    // Verification timeout (converting seconds to milliseconds)
    const actualTimeoutMs = Math.min(Math.max(timeout * 1000, 1000), maxTimeout);
    const timeoutController = new TimeoutController(actualTimeoutMs);

    // Execute a command using TimeoutController
    try {
      const result = await timeoutController.executeWithTimeout(
        () =>
          terminalService.executeOneOff(command, {
            shellType: shell_type,
            cwd: cwd,
            env: env,
            timeout: actualTimeoutMs,
          }),
        actualTimeoutMs,
      );

      // Convert ExecuteResult to ToolOutput
      let content = "";
      if (result.stdout) content += result.stdout;
      if (result.stderr) content += (content ? "\n[stderr]:\n" : "") + result.stderr;
      if (!content) content = "(no output)";

      return {
        success: result.success,
        content,
        error: result.error,
      };
    } catch (error) {
      // Timeout error
      if (error instanceof Error && error.name === "TimeoutError") {
        return {
          success: false,
          content: "",
          error: `Command timed out after ${timeout} seconds`,
        };
      }
      throw error;
    }
  };
}
