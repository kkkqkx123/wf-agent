/**
 * cmd.exe Shell Analyzer — Unit Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { CmdAnalyzer, DANGEROUS_PATTERNS } from "../cmd.js";
import type { ShellPolicy } from "../base.js";

// =========================================================================
// DANGEROUS_PATTERNS
// =========================================================================

describe("DANGEROUS_PATTERNS", () => {
  it("should contain format pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("format"))).toBe(true);
  });

  it("should contain diskpart pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("diskpart"))).toBe(true);
  });

  it("should contain reg import pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("reg\\s+import"))).toBe(true);
  });

  it("should contain powershell pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("powershell"))).toBe(true);
  });

  it("should contain certutil pattern", () => {
    expect(DANGEROUS_PATTERNS.some(p => p.includes("certutil"))).toBe(true);
  });
});

// =========================================================================
// CmdAnalyzer
// =========================================================================

describe("CmdAnalyzer", () => {
  let analyzer: CmdAnalyzer;

  beforeEach(() => {
    analyzer = new CmdAnalyzer();
  });

  describe("identity", () => {
    it("should have correct shellType", () => {
      expect(analyzer.shellType).toBe("cmd");
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

    it("should allow simple echo command", () => {
      const result = analyzer.analyze({
        command: "echo hello",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
      expect(result.shellType).toBe("cmd");
    });

    it("should allow dir command", () => {
      const result = analyzer.analyze({
        command: "dir",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should allow type command", () => {
      const result = analyzer.analyze({
        command: "type file.txt",
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
        allowedCommands: ["echo", "dir"],
        deniedCommands: [],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "format C:",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("not in whitelist");
    });

    it("should deny command in blacklist", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: ["format", "diskpart"],
        dangerousPatterns: [],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "format C:",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("denied by blacklist");
    });

    it("should deny dangerous pattern", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: ["format\\s+[A-Za-z]:"],
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "format C:",
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
        command: "echo hello | findstr test",
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
        command: "echo hello > file.txt",
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
        command: "echo hello | findstr test",
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
        command: "echo hello > file.txt",
        policy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should handle @ prefix (echo suppression)", () => {
      const result = analyzer.analyze({
        command: "@echo off",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should handle start command wrapper", () => {
      const result = analyzer.analyze({
        command: "start /B notepad",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should handle path with backslashes", () => {
      const result = analyzer.analyze({
        command: "C:\\Windows\\System32\\notepad.exe",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should handle .exe extension", () => {
      const result = analyzer.analyze({
        command: "notepad.exe file.txt",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });

    it("should detect format as dangerous", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: DANGEROUS_PATTERNS,
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "format C: /Y",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Dangerous pattern detected");
    });

    it("should detect powershell invocation as dangerous", () => {
      const policy: ShellPolicy = {
        allowedCommands: [],
        deniedCommands: [],
        dangerousPatterns: DANGEROUS_PATTERNS,
        allowPipe: true,
        allowRedirect: true,
      };
      const result = analyzer.analyze({
        command: "powershell -Command Get-Process",
        policy,
      });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain("Dangerous pattern detected");
    });

    it("should return command in result", () => {
      const result = analyzer.analyze({
        command: "echo hello",
        policy: defaultPolicy,
      });
      expect(result.command).toBe("echo hello");
    });

    it("should be case-insensitive for command extraction", () => {
      const result = analyzer.analyze({
        command: "ECHO hello",
        policy: defaultPolicy,
      });
      expect(result.allowed).toBe(true);
    });
  });
});
