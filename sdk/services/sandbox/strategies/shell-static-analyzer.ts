/**
 * Shell Static Analyzer Strategy
 *
 * Lightweight sandbox strategy that performs static analysis on shell commands
 * before delegating to TerminalService for execution.
 *
 * Architecture:
 *   1. Resolve shell type from options.shellType (set by executor from Script.executor.shell)
 *      — fallback: win32→powershell, other→bash
 *   2. Route to shell-specific analyzer for command/cmdlet extraction and pattern matching
 *   3. On success, delegate to TerminalService for execution
 *
 * Architecture reference: docs/infra/sandbox/strategies/shell-static-analyzer.md
 */

import type {
  SandboxPolicy,
  ShellPolicy,
  ScriptExecutionResult,
  StrategyExecuteOptions,
} from "@wf-agent/types";
import type { StrategyImplementation } from "../types.js";
import { getTerminalService, type TerminalService } from "../../terminal/index.js";
import { BashAnalyzer } from "./shell-analyzers/bash.js";
import { PowerShellAnalyzer } from "./shell-analyzers/powershell.js";
import { CmdAnalyzer } from "./shell-analyzers/cmd.js";
import type { ShellAnalyzer, ShellType } from "./shell-analyzers/base.js";

/**
 * Shell Static Analyzer Strategy
 *
 * Performs three layers of static analysis before execution:
 *   Layer 1: Command/cmdlet whitelist/blacklist (shell-specific)
 *   Layer 2: Dangerous pattern regex matching (shell-specific)
 *   Layer 3: Path access & operator checks
 *
 * Shell type is provided by the executor (from Script.executor.shell),
 * not auto-detected from command content.
 */
export class ShellStaticAnalyzerStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "static-analyzer";
  name = "Shell Static Analyzer";
  description = "Static command analysis with shell-type detection and dangerous pattern matching";
  priority = 10;

  private terminalService: TerminalService;
  private analyzerCache = new Map<string, ShellAnalyzer>();

  constructor(terminalService?: TerminalService) {
    this.terminalService = terminalService || getTerminalService();
    this.analyzerCache.set("bash", new BashAnalyzer());
    this.analyzerCache.set("powershell", new PowerShellAnalyzer());
    this.analyzerCache.set("cmd", new CmdAnalyzer());
  }

  isAvailable(): boolean {
    return true;
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const command = options.command;

    if (!command) {
      return this.executionResult(false, startTime, "Empty command");
    }

    // Resolve shell type from executor config, default to platform convention.
    // When runtime is "wsl", always use bash (WSL distro default).
    const shellType = this.resolveShellType(options.shellType, options.runtime);

    // Resolve shell-specific policy
    const shellPolicy: ShellPolicy = {
      allowedCommands: policy.shell?.allowedCommands ?? [],
      deniedCommands: policy.shell?.deniedCommands ?? [],
      dangerousPatterns: policy.shell?.dangerousPatterns ?? [],
      allowPipe: policy.shell?.allowPipe ?? true,
      allowRedirect: policy.shell?.allowRedirect ?? true,
    };

    // Route to shell-specific analyzer
    const analyzer = this.analyzerCache.get(shellType);
    if (!analyzer) {
      return this.deny(command, startTime, `Unsupported shell type: ${shellType}`);
    }

    const result = analyzer.analyze({ command, policy: shellPolicy });
    if (!result.allowed) {
      return this.deny(command, startTime, result.reason ?? "Analysis failed");
    }

    return this.executeCommand(command, options, startTime);
  }

  /**
   * Resolve shell type from options, falling back to platform default.
   *
   * WSL runtime always resolves to bash regardless of shellType,
   * because WSL runs Linux commands through its distro's default shell (bash).
   *
   * Platform convention:
   *   - win32 → "powershell" (most Windows systems have PowerShell pre-installed)
   *   - other → "bash" (Linux/macOS default to bash)
   */
  private resolveShellType(shellType?: ShellType | "auto", runtime?: string): ShellType {
    if (runtime === "wsl") {
      return "bash";
    }
    if (shellType && shellType !== "auto") {
      return shellType;
    }
    return process.platform === "win32" ? "powershell" : "bash";
  }

  /**
   * Execute the command via TerminalService after passing static analysis.
   */
  private async executeCommand(
    command: string,
    options: StrategyExecuteOptions,
    startTime: number,
  ): Promise<ScriptExecutionResult> {
    try {
      const result = await this.terminalService.executeOneOff(command, {
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout,
      });

      return {
        success: result.success,
        scriptName: "sandbox-shell",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-shell",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Build a denial result.
   */
  private deny(command: string, startTime: number, reason: string): ScriptExecutionResult {
    return {
      success: false,
      scriptName: "sandbox-shell",
      executionTime: Date.now() - startTime,
      error: `Sandbox denied execution: ${reason}`,
      stderr: `Command: ${command}\nReason: ${reason}`,
    };
  }

  /**
   * Build an execution result with timing.
   */
  private executionResult(
    success: boolean,
    startTime: number,
    error?: string,
  ): ScriptExecutionResult {
    return {
      success,
      scriptName: "sandbox-shell",
      executionTime: Date.now() - startTime,
      error,
    };
  }
}
