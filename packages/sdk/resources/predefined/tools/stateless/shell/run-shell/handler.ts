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
import { TimeoutController } from "@wf-agent/sdk/services";
import type { ToolOutput, ExecutorShellConfig } from "@wf-agent/types";
import type { ShellPolicy } from "@wf-agent/types";
import type { RunShellConfig } from "../../../types.js";
import { getSandboxRuntime } from "../../../../../../services/sandbox/sandbox-runtime.js";
import { BashAnalyzer } from "../../../../../../services/sandbox/strategies/shell-analyzers/bash.js";
import { CmdAnalyzer } from "../../../../../../services/sandbox/strategies/shell-analyzers/cmd.js";
import { PowerShellAnalyzer } from "../../../../../../services/sandbox/strategies/shell-analyzers/powershell.js";
import type { ShellAnalyzer } from "../../../../../../services/sandbox/strategies/shell-analyzers/base.js";

/**
 * Detect shell type from command for static analysis
 */
function detectShellTypeFromCommand(command: string): "bash" | "powershell" | "cmd" {
  const trimmed = command.trim().toLowerCase();
  if (trimmed.startsWith("powershell") || trimmed.startsWith("pwsh")) return "powershell";
  if (trimmed.startsWith("cmd") || trimmed.startsWith("cmd.exe")) return "cmd";
  return "bash";
}

/**
 * Build shell analyzers map for policy checks
 */
function buildShellAnalyzers(): Map<string, ShellAnalyzer> {
  const analyzers = new Map<string, ShellAnalyzer>();
  analyzers.set("bash", new BashAnalyzer());
  analyzers.set("powershell", new PowerShellAnalyzer());
  analyzers.set("cmd", new CmdAnalyzer());
  return analyzers;
}

/**
 * Run static analysis on a command using ShellPolicy.
 * Returns null if allowed, or an error message if denied.
 */
function runShellPolicyCheck(
  command: string,
  policy: ShellPolicy,
  analyzers: Map<string, ShellAnalyzer>,
): string | null {
  const shellType = detectShellTypeFromCommand(command);
  const analyzer = analyzers.get(shellType);
  if (!analyzer) {
    return `Unsupported shell type for analysis: ${shellType}`;
  }

  const result = analyzer.analyze({ command, policy });
  if (!result.allowed) {
    return result.reason ?? "Command rejected by shell policy";
  }
  return null;
}

/**
 * Create a run_shell tool to execute functions
 */
export function createRunShellHandler(config?: RunShellConfig) {
  const maxTimeout = config?.maxTimeout ?? 600000;
  const terminalService = getTerminalService();
  const shellPolicy = config?.shellPolicy;
  const sandboxConfig = config?.sandboxConfig;
  const analyzers = shellPolicy ? buildShellAnalyzers() : undefined;

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

    if (!command) {
      return {
        success: false,
        content: "",
        error: "No command provided",
      };
    }

    // Phase A: Sandbox policy enforcement via SandboxRuntime
    // Preferred over direct shell policy analysis when sandboxConfig is provided
    if (sandboxConfig) {
      const runtime = getSandboxRuntime();
      if (runtime.isEnabled(sandboxConfig)) {
        const runtimeResult = await runtime.createRuntime(
          "shell",
          {
            command,
            cwd,
            env,
            timeout: timeout * 1000,
            shellType: (shell_type ?? "auto") as ExecutorShellConfig,
          },
          sandboxConfig,
        );

        if (runtimeResult.strategy) {
          const result = await runtimeResult.strategy.execute(
            {
              command,
              cwd,
              env,
              timeout: timeout * 1000,
              shellType: (shell_type ?? "auto") as ExecutorShellConfig,
              vfs: runtimeResult.vfs ?? undefined,
            },
            runtimeResult.policy,
          );

          if (!result.success) {
            return {
              success: false,
              content: "",
              error: result.error ?? "Command rejected by sandbox",
            };
          }

          // Strategy executed successfully, return result
          let content = "";
          if (result.stdout) content += result.stdout;
          if (result.stderr) content += (content ? "\n[stderr]:\n" : "") + result.stderr;
          if (!content) content = "(no output)";

          return {
            success: result.success,
            content,
            error: result.error,
          };
        }
      }
    }

    // Phase B: Direct shell policy pre-check (legacy path, used when no sandboxConfig)
    if (command && shellPolicy && analyzers) {
      const policyError = runShellPolicyCheck(command, shellPolicy, analyzers);
      if (policyError) {
        return {
          success: false,
          content: "",
          error: `Command rejected by shell policy: ${policyError}`,
        };
      }
    }

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
