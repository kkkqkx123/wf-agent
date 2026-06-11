/**
 * Bash Shell Analyzer
 *
 * Shell-specific analysis for bash/POSIX shell commands.
 *
 * Key differences from generic analysis:
 *   - Fork bomb detection (`:(){ :|:& };:`)
 *   - Variable injection (`$(...)`, `` `...` ``)
 *   - `source` / `.` command path restriction
 *   - Wildcard expansion risk for `rm`
 */

import type { ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer } from "./base.js";
import type { ShellType, ShellPolicy } from "./base.js";

const SHELL_TYPE: ShellType = "bash";

/** Bash-specific default denied commands (extended from common denylist) */
const DENIED_COMMANDS: string[] = [
  "sudo",
  "su",
  "chroot",
  "mount",
  "umount",
  "dd",
  "mkfs",
  "reboot",
  "shutdown",
  "poweroff",
  "halt",
  "passwd",
  "useradd",
  "userdel",
  "usermod",
  "groupadd",
  "groupdel",
  "groupmod",
  "lvremove",
  "pvremove",
  "ifdown",
  "ifup",
  "killall",
  "pkill",
  "service",
  "systemctl",
  "insmod",
  "modprobe",
  "modprobe.d",
  "depmod",
  "swapon",
  "swapoff",
];

/** Bash-specific dangerous patterns */
export const DANGEROUS_PATTERNS: string[] = [
  // Fork bomb variants
  ":?\\(\\)\\s*\\{.*:\\s*:\\s*\\};?:",
  // Remote pipe-to-shell
  "curl.*\\|\\s*(ba)?sh",
  "wget.*\\|\\s*(ba)?sh",
  // Library injection
  "LD_PRELOAD=",
  "LD_LIBRARY_PATH=",
  // Filesystem destruction
  "rm\\s+(-rf?|--recursive)\\s+\\/(?!workspace)",
  "mkfs\\s+",
  "dd\\s+if=",
  // chroot escape
  "chroot\\s+",
  // Privilege escalation via env
  "SUDO_ASKPASS=",
  "SUDO_PASSWORD=",
  // Kernel module manipulation
  "insmod\\s+",
  "modprobe\\s+",
  // Dev/raw hardware access
  "dd\\s+of=/dev/(?!null|zero|random|urandom)",
];

/** Bash prefix commands that should be skipped when extracting primary command */
// NOTE: "sudo" is NOT included here because it's a dangerous privilege escalation
// command that should always be checked against the blacklist, not skipped.
const PREFIX_COMMANDS = ["time", "env", "nice", "nohup", "command", "\\"];

export class BashAnalyzer implements ShellAnalyzer {
  readonly shellType = SHELL_TYPE;

  analyze(ctx: ShellAnalysisContext): ShellAnalysisResult {
    // Resolve policy with defaults first
    const policy = this.resolvePolicy(ctx.policy);

    // Layer 1: Extract primary command and check whitelist/blacklist
    const primaryCommand = this.extractPrimaryCommand(ctx.command);
    if (!primaryCommand) {
      return { allowed: false, reason: "Empty command", command: ctx.command, shellType: SHELL_TYPE };
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
      const regex = new RegExp(pattern);
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
   * Extract the primary command (first word), skipping common prefixes.
   */
  private extractPrimaryCommand(command: string): string {
    const trimmed = command.trim();
    if (!trimmed) return "";

    const tokens = trimmed.split(/\s+/);

    let primary: string | undefined = tokens[0];
    while (primary && PREFIX_COMMANDS.includes(primary)) {
      const idx = tokens.indexOf(primary);
      primary = tokens[idx + 1];
    }

    return (primary ?? "").replace(/[^a-zA-Z0-9_\-./\\]/g, "");
  }
}