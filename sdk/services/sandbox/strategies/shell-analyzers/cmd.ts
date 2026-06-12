/**
 * cmd.exe Shell Analyzer
 *
 * Shell-specific analysis for Windows cmd.exe commands.
 *
 * cmd.exe attack surface differs significantly from bash/PowerShell:
 *   - No fork bombs, but `format C:\ /Y` is destructive
 *   - `%PATH%` vs `$PATH` variable syntax
 *   - `!VAR!` delayed expansion syntax
 *   - Batch file execution with `.bat`/`.cmd`
 *   - No pipe-to-shell equivalent, but `|` pipes text
 *   - `start` command for process spawning
 *   - `assoc`/`ftype` manipulation
 */

import type { ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer } from "./base.js";
import type { ShellType, ShellPolicy } from "./base.js";

const SHELL_TYPE: ShellType = "cmd";

/** cmd.exe-specific default denied commands */
const DENIED_COMMANDS: string[] = [
  // System manipulation
  "format",
  "diskpart",
  "diskcomp",
  "diskcopy",
  "fdisk",
  // Privilege
  "runas",
  // Registry
  "reg",
  "regedit",
  "regedt32",
  "regini",
  // Network/shares
  "net",
  "net1",
  "netsh",
  // System config
  "bcdedit",
  "bootcfg",
  "bootsect",
  // WMIC (deprecated but still available)
  "wmic",
  // File association
  "assoc",
  "ftype",
  // Process
  "taskkill",
  "tskill",
];

/** cmd.exe-specific dangerous patterns */
export const DANGEROUS_PATTERNS: string[] = [
  // Format disk
  "format\\s+[A-Za-z]:",
  "format\\s+/",
  // Diskpart destructive commands
  "diskpart\\s+/s",
  "clean\\s+all",
  // Registry manipulation via reg import
  "reg\\s+import",
  "reg\\s+add",
  "reg\\s+delete",
  // Dangerous WMIC
  "wmic\\s+process\\s+delete",
  "wmic\\s+path\\s+",
  // Network share manipulation
  "net\\s+share",
  "net\\s+use",
  // Remote execution
  "psexec",
  "winrm",
  // Batch download & exec
  "bitsadmin\\s+/transfer",
  "certutil\\s+-urlcache",
  "certutil\\s+-decode",
  "cscript\\s+",
  // COM/shell object via mshta
  "mshta\\s+",
  // PowerShell invoked from cmd
  "powershell\\s+",
  "pwsh\\s+",
];

export class CmdAnalyzer implements ShellAnalyzer {
  readonly shellType = SHELL_TYPE;

  analyze(ctx: ShellAnalysisContext): ShellAnalysisResult {
    // Resolve policy with defaults first
    const policy = this.resolvePolicy(ctx.policy);

    // Layer 1: Extract primary command
    const primaryCommand = this.extractPrimaryCommand(ctx.command);
    if (!primaryCommand) {
      return {
        allowed: false,
        reason: "Empty command",
        command: ctx.command,
        shellType: SHELL_TYPE,
      };
    }

    if (policy.allowedCommands.length > 0 && !policy.allowedCommands.includes(primaryCommand)) {
      return {
        allowed: false,
        reason: `Command not in whitelist: ${primaryCommand}`,
        command: ctx.command,
        shellType: SHELL_TYPE,
      };
    }
    if (policy.deniedCommands.includes(primaryCommand)) {
      return {
        allowed: false,
        reason: `Command denied by blacklist: ${primaryCommand}`,
        command: ctx.command,
        shellType: SHELL_TYPE,
      };
    }

    // Layer 2: Dangerous pattern detection
    for (const pattern of policy.dangerousPatterns) {
      const regex = new RegExp(pattern, "i");
      if (regex.test(ctx.command)) {
        return {
          allowed: false,
          reason: `Dangerous pattern detected: ${pattern}`,
          command: ctx.command,
          shellType: SHELL_TYPE,
        };
      }
    }

    // Layer 3: Operator checks
    if (!policy.allowPipe && ctx.command.includes("|")) {
      return {
        allowed: false,
        reason: "Pipe operator is not allowed",
        command: ctx.command,
        shellType: SHELL_TYPE,
      };
    }
    if (!policy.allowRedirect && /[<>]/.test(ctx.command)) {
      return {
        allowed: false,
        reason: "Redirect operator is not allowed",
        command: ctx.command,
        shellType: SHELL_TYPE,
      };
    }

    return { allowed: true, command: ctx.command, shellType: SHELL_TYPE };
  }

  private resolvePolicy(policy: ShellPolicy): Required<ShellPolicy> {
    return {
      allowedCommands: policy.allowedCommands ?? [],
      deniedCommands: policy.deniedCommands ?? DENIED_COMMANDS,
      dangerousPatterns: policy.dangerousPatterns ?? DANGEROUS_PATTERNS,
      allowPipe: policy.allowPipe ?? true,
      allowRedirect: policy.allowRedirect ?? true,
    };
  }

  /**
   * Extract primary command from cmd.exe command line.
   *
   * Handles:
   *   `command /option args...`
   *   `@command /option args...`  (suppress echo)
   *   `start /B command`          (start wrapper)
   *   `path\to\command.exe`       (with path)
   */
  private extractPrimaryCommand(command: string): string {
    const trimmed = command.trim();
    if (!trimmed) return "";

    // Remove leading @ (echo suppression)
    const noAt = trimmed.replace(/^@\s*/, "");

    // Skip `start` wrapper
    const noStart = noAt.replace(/^start\s+(\/[A-Za-z]\s+)*/, "");

    // Get first token
    const firstToken = noStart.split(/\s+/)[0];
    if (!firstToken) return "";

    // Extract basename from path (handle both \ and /)
    const basename = firstToken.replace(/^.*[/\\]/, "").replace(/\.(exe|com|bat|cmd)$/i, "");

    return basename.toLowerCase();
  }
}
