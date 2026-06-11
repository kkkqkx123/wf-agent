/**
 * Shell Analyzer Base — Unit Tests
 */

import { describe, it, expect } from "vitest";
import type { ShellAnalysisResult, ShellAnalysisContext, ShellAnalyzer, ShellType, ShellPolicy } from "../base.js";

// =========================================================================
// Type Exports
// =========================================================================

describe("Shell Analyzer Base Types", () => {
  it("should export ShellType as string union", () => {
    const shellTypes: ShellType[] = ["bash", "powershell", "cmd"];
    expect(shellTypes).toHaveLength(3);
  });

  it("should define ShellAnalysisResult interface", () => {
    const result: ShellAnalysisResult = {
      allowed: true,
      command: "echo hello",
      shellType: "bash",
    };
    expect(result.allowed).toBe(true);
    expect(result.command).toBe("echo hello");
    expect(result.shellType).toBe("bash");
  });

  it("should define ShellAnalysisResult with optional reason", () => {
    const result: ShellAnalysisResult = {
      allowed: false,
      reason: "Command denied",
      command: "rm -rf /",
      shellType: "bash",
    };
    expect(result.reason).toBe("Command denied");
  });

  it("should define ShellAnalysisContext interface", () => {
    const policy: ShellPolicy = {
      allowedCommands: ["echo", "ls"],
      deniedCommands: ["rm"],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: true,
    };
    const ctx: ShellAnalysisContext = {
      command: "echo hello",
      policy,
    };
    expect(ctx.command).toBe("echo hello");
    expect(ctx.policy.allowedCommands).toContain("echo");
  });

  it("should define ShellAnalyzer interface", () => {
    const analyzer: ShellAnalyzer = {
      shellType: "bash",
      analyze: (ctx: ShellAnalysisContext): ShellAnalysisResult => {
        return {
          allowed: true,
          command: ctx.command,
          shellType: "bash",
        };
      },
    };
    expect(analyzer.shellType).toBe("bash");
    expect(typeof analyzer.analyze).toBe("function");
  });
});

// =========================================================================
// ShellPolicy
// =========================================================================

describe("ShellPolicy", () => {
  it("should support empty allowedCommands", () => {
    const policy: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: [],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: true,
    };
    expect(policy.allowedCommands).toHaveLength(0);
  });

  it("should support allowedCommands list", () => {
    const policy: ShellPolicy = {
      allowedCommands: ["git", "npm", "node"],
      deniedCommands: [],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: true,
    };
    expect(policy.allowedCommands).toContain("git");
    expect(policy.allowedCommands).toContain("npm");
    expect(policy.allowedCommands).toContain("node");
  });

  it("should support deniedCommands list", () => {
    const policy: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: ["rm", "sudo", "chmod"],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: true,
    };
    expect(policy.deniedCommands).toContain("rm");
    expect(policy.deniedCommands).toContain("sudo");
    expect(policy.deniedCommands).toContain("chmod");
  });

  it("should support dangerousPatterns list", () => {
    const policy: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: [],
      dangerousPatterns: ["rm\\s+-rf\\s+/", "curl.*\\|\\s*bash"],
      allowPipe: true,
      allowRedirect: true,
    };
    expect(policy.dangerousPatterns).toHaveLength(2);
  });

  it("should support allowPipe flag", () => {
    const policyWithPipe: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: [],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: true,
    };
    const policyWithoutPipe: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: [],
      dangerousPatterns: [],
      allowPipe: false,
      allowRedirect: true,
    };
    expect(policyWithPipe.allowPipe).toBe(true);
    expect(policyWithoutPipe.allowPipe).toBe(false);
  });

  it("should support allowRedirect flag", () => {
    const policyWithRedirect: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: [],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: true,
    };
    const policyWithoutRedirect: ShellPolicy = {
      allowedCommands: [],
      deniedCommands: [],
      dangerousPatterns: [],
      allowPipe: true,
      allowRedirect: false,
    };
    expect(policyWithRedirect.allowRedirect).toBe(true);
    expect(policyWithoutRedirect.allowRedirect).toBe(false);
  });
});
