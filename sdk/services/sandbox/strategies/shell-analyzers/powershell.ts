/**
 * PowerShell Shell Analyzer
 *
 * Shell-specific analysis for PowerShell commands.
 *
 * PowerShell's unique attack surface:
 *   - Cmdlet system (`Invoke-Expression`, `Invoke-Command`, `Get-WmiObject`)
 *   - `IEX` (alias for Invoke-Expression) — remote code download & exec
 *   - `New-Object Net.WebClient` — web requests
 *   - `-EncodedCommand` — encoded payload execution
 *   - `Out-File` / `Set-Content` — file writes
 *   - `Start-Process` — process spawning
 *   - AMSI bypass patterns
 *   - PowerShell Remoting (`Enter-PSSession`, `Invoke-Command -ComputerName`)
 */

import type { ShellAnalysisContext, ShellAnalysisResult, ShellAnalyzer } from "./base.js";
import type { ShellType } from "./base.js";

const SHELL_TYPE: ShellType = "powershell";

/** PowerShell-specific default denied commands */
const DENIED_COMMANDS: string[] = [
  // Process/WMI/COM manipulation
  "Start-Process",
  "Stop-Process",
  "Get-WmiObject",
  "Get-WinEvent",
  "Invoke-WmiMethod",
  "Register-WmiEvent",
  "Remove-WmiObject",
  "Set-WmiInstance",
  // Code execution
  "Invoke-Expression",
  "Invoke-Command",
  "Invoke-CimMethod",
  // File download
  "Invoke-WebRequest",
  "Invoke-RestMethod",
  // Remoting
  "Enter-PSSession",
  "Exit-PSSession",
  "New-PSSession",
  "Remove-PSSession",
  // System manipulation
  "Set-ExecutionPolicy",
  "Set-MpPreference",
  "Unblock-File",
  // Service management
  "New-Service",
  "Set-Service",
  "Restart-Service",
  "Stop-Service",
  // Registry
  "New-ItemProperty",
  "Set-ItemProperty",
  "Remove-ItemProperty",
];

/** PowerShell-specific dangerous patterns */
const DANGEROUS_PATTERNS: string[] = [
  // IEX — the most common PowerShell attack vector
  "IEX\\s*\\(?\\s*(New-Object|Invoke-WebRequest|Invoke-RestMethod)",
  "Invoke-Expression\\s*\\(?\\s*(New-Object|Invoke-WebRequest)",
  // Encoded command execution
  "-EncodedCommand\\s+",
  "-e\\s+[A-Za-z0-9+/=]{20,}",
  "\\[System\\.Convert\\]::FromBase64String",
  // Web client download
  "New-Object\\s+Net\\.WebClient",
  "New-Object\\s+System\\.Net\\.WebClient",
  "\\.DownloadString\\(\\s*(['\"]?)https?://",
  "\\.DownloadFile\\(\\s*(['\"]?)https?://",
  // AMSI bypass
  "AmsiUtils",
  "amsiInitFailed",
  "System.Management.Automation.AmsiUtils",
  "\[Ref\].*Assembly.*Load.*System\\.Management\\.Automation",
  // Reflection-based bypass
  "GetField\\s*\\(\\s*['\"]amsi",
  "SetValue\\s*\\(\\s*null",
  // COM object creation
  "New-Object\\s+-ComObject\\s+",
  "CreateObject\\s*\\(\\s*['\"]WScript\\.Shell",
  // Token manipulation
  "Advapi32\\..*OpenProcessToken",
  "Advapi32\\..*DuplicateToken",
  // Kernel32
  "Kernel32\\..*VirtualAlloc",
  "Kernel32\\..*CreateThread",
  "Kernel32\\..*CreateProcess",
];

/** Denied cmdlet alias resolution map */
const CMDLET_ALIASES: Record<string, string> = {
  "iex": "Invoke-Expression",
  "iwr": "Invoke-WebRequest",
  "irm": "Invoke-RestMethod",
  "icm": "Invoke-Command",
  "saps": "Start-Process",
  "gcm": "Get-Command",
  "gm": "Get-Member",
  "gi": "Get-Item",
  "gci": "Get-ChildItem",
  "gl": "Get-Location",
  "gp": "Get-ItemProperty",
  "gsv": "Get-Service",
  "gwm": "Get-WmiObject",
  "ni": "New-Item",
  "nv": "New-Variable",
  "ogv": "Out-GridView",
  "oh": "Out-Host",
  "r": "Invoke-History",
  "rc": "Set-PSReadLineOption",
  "rm": "Remove-Item",
  "rmdir": "Remove-Item",
  "sasv": "Start-Service",
  "shcm": "Show-Command",
  "sls": "Select-String",
  "sp": "Set-ItemProperty",
  "spsv": "Stop-Service",
  "sv": "Set-Variable",
  "tee": "Tee-Object",
  "type": "Get-Content",
  "wi": "Write-Output",
  "write": "Write-Output",
};

export class PowerShellAnalyzer implements ShellAnalyzer {
  readonly shellType = SHELL_TYPE;

  analyze(ctx: ShellAnalysisContext): ShellAnalysisResult {
    const policy = this.resolvePolicy(ctx.policy);

    // Layer 1: Extract primary cmdlet and check whitelist/blacklist
    const primaryCmdlet = this.extractPrimaryCmdlet(ctx.command);
    if (!primaryCmdlet) {
      return { allowed: false, reason: "Empty command", command: ctx.command, shellType: SHELL_TYPE };
    }

    if (policy.allowedCommands.length > 0 && !policy.allowedCommands.includes(primaryCmdlet)) {
      return {
        allowed: false,
        reason: `Command not in whitelist: ${primaryCmdlet}`,
        command: ctx.command,
        shellType: SHELL_TYPE,
      };
    }
    if (policy.deniedCommands.includes(primaryCmdlet)) {
      return {
        allowed: false,
        reason: `Command denied by blacklist: ${primaryCmdlet}`,
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

  private resolvePolicy(policy: ShellPolicy): ShellPolicy {
    return {
      allowedCommands: policy.allowedCommands ?? [],
      deniedCommands: policy.deniedCommands ?? DENIED_COMMANDS,
      dangerousPatterns: policy.dangerousPatterns ?? DANGEROUS_PATTERNS,
      allowPipe: policy.allowPipe ?? true,
      allowRedirect: policy.allowRedirect ?? true,
    };
  }

  /**
   * Extract primary cmdlet from a PowerShell command.
   *
   * PowerShell commands can be:
   *   `Get-Process`                — full cmdlet name
   *   `iex "..."`                  — alias
   *   `gci | Where-Object ...`     — pipeline with multiple cmds
   *   `$var = Get-Process; ...`    — variable assignment
   *   `& "some.exe"`               — call operator with exe
   *
   * Returns the resolved cmdlet name (alias → full name) or the first executable.
   */
  private extractPrimaryCmdlet(command: string): string {
    const trimmed = command.trim();
    if (!trimmed) return "";

    // Remove variable assignment prefix: `$var = cmdlet ...`
    const withoutAssignment = trimmed.replace(/^\$\w+\s*=\s*/, "");

    // Remove leading call operator: `& cmdlet ...`
    const withoutCallOp = withoutAssignment.replace(/^&\s*/, "");

    // Get first token (split by space, semicolon, pipe)
    const firstToken = withoutCallOp.split(/[\s|;]+/)[0];
    if (!firstToken) return "";

    // Resolve alias
    const aliasKey = firstToken.toLowerCase();
    if (CMDLET_ALIASES[aliasKey]) {
      return CMDLET_ALIASES[aliasKey]!;
    }

    // Clean up path separators and quotes
    return firstToken.replace(/['"]/g, "");
  }
}