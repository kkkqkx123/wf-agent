/**
 * Auto Approval Checker Unit Tests
 */

import { describe, it, expect } from "vitest";
import { checkAutoApproval, extractContextFromParameters } from "../auto-approval-checker.js";
import type {
  Tool,
  ToolApprovalOptions,
  AutoApprovalContext,
  McpRequest,
  FilePermissionSettings,
} from "@wf-agent/types";

// ============================================================================
// Helpers
// ============================================================================
function makeTool(overrides?: Partial<Tool>): Tool {
  return {
    id: "read_file",
    type: "STATELESS" as const,
    description: "Test tool",
    parameters: { type: "object", properties: {}, required: [] },
    metadata: { riskLevel: "READ_ONLY" },
    ...overrides,
  };
}

function makeOptions(overrides?: Partial<ToolApprovalOptions>): ToolApprovalOptions {
  return {
    autoApprovalEnabled: true,
    ...overrides,
  };
}

function makeContext(overrides?: Partial<AutoApprovalContext>): AutoApprovalContext {
  return {
    ...overrides,
  };
}

// ============================================================================
// checkAutoApproval — Basic flow
// ============================================================================
describe("checkAutoApproval", () => {
  it("should ask when auto-approval is not enabled", () => {
    const result = checkAutoApproval({
      options: { autoApprovalEnabled: false },
      tool: makeTool(),
      context: makeContext(),
    });
    expect(result).toEqual({ decision: "ask" });
  });

  it("should ask when options is empty (autoApprovalEnabled undefined)", () => {
    const result = checkAutoApproval({
      options: {},
      tool: makeTool(),
      context: makeContext(),
    });
    expect(result).toEqual({ decision: "ask" });
  });

  // ============================================================================
  // Security preset
  // ============================================================================
  describe("security preset application", () => {
    it("should apply SAFE preset and deny write by default", () => {
      const result = checkAutoApproval({
        options: makeOptions({ securityPreset: "SAFE" }),
        tool: makeTool({ metadata: { riskLevel: "WRITE" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should apply BALANCED preset and allow read-only", () => {
      const result = checkAutoApproval({
        options: makeOptions({ securityPreset: "BALANCED" }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should apply PERMISSIVE preset and allow execute with command in allowlist", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          securityPreset: "PERMISSIVE",
          command: { allowedCommands: ["echo"] },
        }),
        tool: makeTool({ metadata: { riskLevel: "EXECUTE" } }),
        context: makeContext({ command: "echo hello" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should use invalid preset name gracefully (falls back to no preset)", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          securityPreset: "INVALID" as never,
          categories: { alwaysAllowReadOnly: true },
        }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "approve" });
    });
  });

  // ============================================================================
  // File permission check (HIGHEST PRIORITY)
  // ============================================================================
  describe("file permission check (highest priority)", () => {
    const filePermissions: FilePermissionSettings = {
      rules: [{ pattern: "**/secret-file.yml", permission: "denied" }],
      defaultPermission: "write",
    };

    it("should deny when file permission check fails", () => {
      const result = checkAutoApproval({
        options: makeOptions({ filePermissions }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext({ filePath: "/workspace/secret-file.yml", fileOperation: "read" }),
      });
      expect(result.decision).toBe("deny");
      expect(result.decision === "deny" && result.reason).toBeTruthy();
    });

    it("should continue to next checks when file permission passes", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          filePermissions,
          categories: { alwaysAllowReadOnly: true },
        }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext({ filePath: "/workspace/src/index.ts", fileOperation: "read" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should skip file permission check when no filePath in context", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          filePermissions,
          categories: { alwaysAllowReadOnly: true },
        }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should use 'read' as default file operation when not specified", () => {
      const result = checkAutoApproval({
        options: makeOptions({ filePermissions }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext({ filePath: "/workspace/secret-file.yml" }),
      });
      expect(result.decision).toBe("deny");
    });
  });

  // ============================================================================
  // Risk level checks
  // ============================================================================
  describe("risk level: SYSTEM", () => {
    it("should never auto-approve SYSTEM level", () => {
      const result = checkAutoApproval({
        options: makeOptions({ categories: { alwaysAllowExecute: true } }),
        tool: makeTool({ metadata: { riskLevel: "SYSTEM" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("autoApprovable flag", () => {
    it("should approve when autoApprovable is true (overrides risk level)", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "WRITE", autoApprovable: true } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should NOT approve SYSTEM tools even with autoApprovable (SYSTEM has highest restriction)", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "SYSTEM", autoApprovable: true } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when autoApprovable is false", () => {
      const result = checkAutoApproval({
        options: makeOptions({ categories: { alwaysAllowReadOnly: true } }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY", autoApprovable: false } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("risk level: READ_ONLY", () => {
    it("should approve when alwaysAllowReadOnly is true", () => {
      const result = checkAutoApproval({
        options: makeOptions({ categories: { alwaysAllowReadOnly: true } }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should ask when alwaysAllowReadOnly is false/undefined", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when outside workspace and boundary not allowed", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowReadOnly: true },
          workspaceBoundary: { allowReadOnlyOutsideWorkspace: false },
        }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext({ isOutsideWorkspace: true }),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should approve when outside workspace and boundary allowed", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowReadOnly: true },
          workspaceBoundary: { allowReadOnlyOutsideWorkspace: true },
        }),
        tool: makeTool({ metadata: { riskLevel: "READ_ONLY" } }),
        context: makeContext({ isOutsideWorkspace: true }),
      });
      expect(result).toEqual({ decision: "approve" });
    });
  });

  describe("risk level: WRITE", () => {
    it("should approve when alwaysAllowWrite is true", () => {
      const result = checkAutoApproval({
        options: makeOptions({ categories: { alwaysAllowWrite: true } }),
        tool: makeTool({ metadata: { riskLevel: "WRITE" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should ask when alwaysAllowWrite is false", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "WRITE" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when outside workspace and boundary not allowed", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowWrite: true },
          workspaceBoundary: { allowWriteOutsideWorkspace: false },
        }),
        tool: makeTool({ metadata: { riskLevel: "WRITE" } }),
        context: makeContext({ isOutsideWorkspace: true }),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when file is protected and allowWriteProtected is false", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowWrite: true },
          allowWriteProtected: false,
        }),
        tool: makeTool({ metadata: { riskLevel: "WRITE" } }),
        context: makeContext({ isProtected: true }),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should approve protected file when allowWriteProtected is true", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowWrite: true },
          allowWriteProtected: true,
        }),
        tool: makeTool({ metadata: { riskLevel: "WRITE" } }),
        context: makeContext({ isProtected: true }),
      });
      expect(result).toEqual({ decision: "approve" });
    });
  });

  describe("risk level: EXECUTE", () => {
    it("should approve with command in allowlist", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowExecute: true },
          command: { allowedCommands: ["git"] },
        }),
        tool: makeTool({ metadata: { riskLevel: "EXECUTE" } }),
        context: makeContext({ command: "git status" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should deny with command in denylist", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowExecute: true },
          command: { allowedCommands: ["git"], deniedCommands: ["git push"] },
        }),
        tool: makeTool({ metadata: { riskLevel: "EXECUTE" } }),
        context: makeContext({ command: "git push origin main" }),
      });
      expect(result).toEqual({ decision: "deny", reason: expect.any(String) });
    });

    it("should ask when command not in allowlist", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowExecute: true },
          command: { allowedCommands: ["git"] },
        }),
        tool: makeTool({ metadata: { riskLevel: "EXECUTE" } }),
        context: makeContext({ command: "npm install" }),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when command is missing", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowExecute: true },
        }),
        tool: makeTool({ metadata: { riskLevel: "EXECUTE" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when alwaysAllowExecute is false", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "EXECUTE" } }),
        context: makeContext({ command: "git status" }),
      });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("risk level: MCP", () => {
    const mcpRequest: McpRequest = {
      type: "use_mcp",
      serverName: "test-server",
      toolName: "test-tool",
      arguments: {},
    };

    it("should approve with alwaysAllowMcp and no MCP settings", () => {
      const result = checkAutoApproval({
        options: makeOptions({ categories: { alwaysAllowMcp: true } }),
        tool: makeTool({ metadata: { riskLevel: "MCP" } }),
        context: makeContext({ mcpRequest }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should use fine-grained MCP settings when provided", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowMcp: true },
          mcp: {
            servers: [{ name: "test-server", tools: [{ name: "test-tool", alwaysAllow: true }] }],
          },
        }),
        tool: makeTool({ metadata: { riskLevel: "MCP" } }),
        context: makeContext({ mcpRequest }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should deny with fine-grained MCP settings when denied", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowMcp: true },
          mcp: {
            servers: [{ name: "test-server", defaultToolBehavior: "always_deny" }],
          },
        }),
        tool: makeTool({ metadata: { riskLevel: "MCP" } }),
        context: makeContext({ mcpRequest }),
      });
      expect(result).toEqual({ decision: "deny", reason: expect.any(String) });
    });

    it("should ask when alwaysAllowMcp is false", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "MCP" } }),
        context: makeContext({ mcpRequest }),
      });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("risk level: NETWORK", () => {
    it("should approve domain in allowlist", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowNetwork: true },
          network: { allowedDomains: ["example.com"] },
        }),
        tool: makeTool({ metadata: { riskLevel: "NETWORK" } }),
        context: makeContext({ domain: "example.com" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should deny domain in denylist", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowNetwork: true },
          network: { allowedDomains: ["*"], deniedDomains: ["bad.example.com"] },
        }),
        tool: makeTool({ metadata: { riskLevel: "NETWORK" } }),
        context: makeContext({ domain: "bad.example.com" }),
      });
      expect(result).toEqual({ decision: "deny", reason: expect.any(String) });
    });

    it("should ask when domain not in allowlist (with non-wildcard)", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowNetwork: true },
          network: { allowedDomains: ["example.com"] },
        }),
        tool: makeTool({ metadata: { riskLevel: "NETWORK" } }),
        context: makeContext({ domain: "other.com" }),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when domain is missing", () => {
      const result = checkAutoApproval({
        options: makeOptions({ categories: { alwaysAllowNetwork: true } }),
        tool: makeTool({ metadata: { riskLevel: "NETWORK" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("risk level: INTERACTION", () => {
    it("should timeout with autoResponse when configured", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowInteraction: true },
          interaction: { followupAutoApproveTimeoutMs: 5000 },
        }),
        tool: makeTool({ metadata: { riskLevel: "INTERACTION" } }),
        context: makeContext({ followupSuggestion: "yes" }),
      });
      expect(result).toEqual({
        decision: "timeout",
        timeout: 5000,
        autoResponse: "yes",
      });
    });

    it("should ask when alwaysAllowInteraction is false", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "INTERACTION" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when no followupSuggestion", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowInteraction: true },
          interaction: { followupAutoApproveTimeoutMs: 5000 },
        }),
        tool: makeTool({ metadata: { riskLevel: "INTERACTION" } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should ask when timeout is 0", () => {
      const result = checkAutoApproval({
        options: makeOptions({
          categories: { alwaysAllowInteraction: true },
          interaction: { followupAutoApproveTimeoutMs: 0 },
        }),
        tool: makeTool({ metadata: { riskLevel: "INTERACTION" } }),
        context: makeContext({ followupSuggestion: "yes" }),
      });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("default risk level fallback", () => {
    it("should fall back to WRITE when risk level is unknown", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: { riskLevel: "UNKNOWN" as never } }),
        context: makeContext(),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should handle undefined metadata", () => {
      const result = checkAutoApproval({
        options: makeOptions(),
        tool: makeTool({ metadata: undefined }),
        context: makeContext(),
      });
      // Default risk level is WRITE from classification, which requires category
      expect(result).toEqual({ decision: "ask" });
    });
  });
});

// ============================================================================
// extractContextFromParameters
// ============================================================================
describe("extractContextFromParameters", () => {
  describe("file tools", () => {
    it("should extract filePath and fileOperation for read_file", () => {
      const context = extractContextFromParameters("read_file", { path: "/test/file.txt" });
      expect(context.filePath).toBe("/test/file.txt");
      expect(context.fileOperation).toBe("read");
    });

    it("should extract filePath and fileOperation for write_file", () => {
      const context = extractContextFromParameters("write_file", { path: "/test/file.txt" });
      expect(context.filePath).toBe("/test/file.txt");
      expect(context.fileOperation).toBe("write");
    });

    it("should extract filePath and fileOperation for edit", () => {
      const context = extractContextFromParameters("edit", { path: "/test/file.txt" });
      expect(context.filePath).toBe("/test/file.txt");
      expect(context.fileOperation).toBe("write");
    });

    it("should extract filePath and fileOperation for apply_diff", () => {
      const context = extractContextFromParameters("apply_diff", { path: "/test/file.txt" });
      expect(context.filePath).toBe("/test/file.txt");
      expect(context.fileOperation).toBe("write");
    });

    it("should throw error when path is missing for file tool", () => {
      expect(() => extractContextFromParameters("read_file", {})).toThrow(
        "Invalid or missing 'path' parameter",
      );
    });

    it("should throw error when path is not a string", () => {
      expect(() => extractContextFromParameters("read_file", { path: 123 })).toThrow(
        "Invalid or missing 'path' parameter",
      );
    });
  });

  describe("shell tools", () => {
    it("should extract command for run_shell", () => {
      const context = extractContextFromParameters("run_shell", { command: "echo hello" });
      expect(context.command).toBe("echo hello");
    });

    it("should extract command for backend_shell", () => {
      const context = extractContextFromParameters("backend_shell", { command: "npm start" });
      expect(context.command).toBe("npm start");
    });

    it("should extract command for run_slash_command", () => {
      const context = extractContextFromParameters("run_slash_command", { command: "/help" });
      expect(context.command).toBe("/help");
    });

    it("should throw error when command is missing for shell tool", () => {
      expect(() => extractContextFromParameters("run_shell", {})).toThrow(
        "Invalid or missing 'command' parameter",
      );
    });
  });

  describe("MCP tools", () => {
    it("should extract MCP request for use_mcp", () => {
      const context = extractContextFromParameters("use_mcp", {
        server_name: "my-server",
        tool_name: "my-tool",
        arguments: { key: "value" },
      });
      expect(context.mcpRequest).toBeDefined();
      expect(context.mcpRequest!.type).toBe("use_mcp");
      expect(context.mcpRequest!.serverName).toBe("my-server");
      expect(context.mcpRequest!.toolName).toBe("my-tool");
      expect(context.mcpRequest!.arguments).toEqual({ key: "value" });
    });

    it("should default arguments to empty object when not provided", () => {
      const context = extractContextFromParameters("use_mcp", {
        server_name: "my-server",
        tool_name: "my-tool",
      });
      expect(context.mcpRequest!.arguments).toEqual({});
    });

    it("should throw error when server_name is missing", () => {
      expect(() =>
        extractContextFromParameters("use_mcp", { tool_name: "my-tool" }),
      ).toThrow("Invalid parameters for MCP tool");
    });

    it("should throw error when tool_name is not a string", () => {
      expect(() =>
        extractContextFromParameters("use_mcp", { server_name: "my-server", tool_name: 123 }),
      ).toThrow("Invalid parameters for MCP tool");
    });
  });

  describe("non-matching tools", () => {
    it("should return empty context for unknown tools", () => {
      const context = extractContextFromParameters("unknown_tool", { some: "param" });
      expect(context).toEqual({});
    });

    it("should return empty context for non-file/shell/MCP tools", () => {
      const context = extractContextFromParameters("query_workflow_status", { workflowId: "123" });
      expect(context).toEqual({});
    });
  });
});