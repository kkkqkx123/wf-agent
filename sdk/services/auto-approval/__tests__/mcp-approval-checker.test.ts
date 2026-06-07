/**
 * MCP Approval Checker Unit Tests
 */

import { describe, it, expect } from "vitest";
import {
  checkMcpApproval,
  createDefaultMcpApprovalSettings,
  mergeMcpApprovalSettings,
  isServerConfigured,
  getAutoApprovedTools,
} from "../mcp-approval-checker.js";
import type {
  McpApprovalSettings,
  McpApprovalServerConfig,
  McpToolCallRequest,
  McpResourceReadRequest,
  McpListRequest,
} from "@wf-agent/types";

// ============================================================================
// Helper factories
// ============================================================================
function makeToolCall(overrides?: Partial<McpToolCallRequest>): McpToolCallRequest {
  return {
    type: "use_mcp",
    serverName: "test-server",
    toolName: "test-tool",
    arguments: {},
    ...overrides,
  };
}

function makeResourceRead(overrides?: Partial<McpResourceReadRequest>): McpResourceReadRequest {
  return {
    type: "read_resource",
    serverName: "test-server",
    uri: "file:///test.txt",
    ...overrides,
  };
}

function makeListRequest(overrides?: Partial<McpListRequest>): McpListRequest {
  return {
    type: "list_tools",
    serverName: "test-server",
    ...overrides,
  };
}

function makeSettings(servers: McpApprovalServerConfig[] = []): McpApprovalSettings {
  return { servers, defaultServerBehavior: "always_ask" };
}

// ============================================================================
// checkMcpApproval
// ============================================================================
describe("checkMcpApproval", () => {
  describe("unknown server handling", () => {
    it("should ask when server is unknown and defaultServerBehavior is always_ask", () => {
      const result = checkMcpApproval({
        settings: makeSettings(),
        request: makeToolCall({ serverName: "unknown" }),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should deny when server is unknown and defaultServerBehavior is always_deny", () => {
      const result = checkMcpApproval({
        settings: {
          servers: [],
          defaultServerBehavior: "always_deny",
        },
        request: makeToolCall({ serverName: "unknown" }),
      });
      expect(result).toEqual({ decision: "deny", reason: expect.stringContaining("unknown") });
    });
  });

  describe("tool call requests", () => {
    it("should approve tool explicitly configured with alwaysAllow", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          tools: [{ name: "test-tool", alwaysAllow: true }],
        },
      ]);
      const result = checkMcpApproval({ settings, request: makeToolCall() });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should approve tool with READ_ONLY risk level override", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          tools: [{ name: "test-tool", riskLevel: "READ_ONLY" }],
        },
      ]);
      const result = checkMcpApproval({ settings, request: makeToolCall() });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should ask for tool explicitly configured but not auto-approved", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          tools: [{ name: "test-tool" }],
        },
      ]);
      const result = checkMcpApproval({ settings, request: makeToolCall() });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should use defaultToolBehavior when tool not configured", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          defaultToolBehavior: "always_approve",
        },
      ]);
      const result = checkMcpApproval({ settings, request: makeToolCall() });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should deny when defaultToolBehavior is always_deny", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          defaultToolBehavior: "always_deny",
        },
      ]);
      const result = checkMcpApproval({ settings, request: makeToolCall() });
      expect(result).toEqual({ decision: "deny", reason: expect.stringContaining("not in allowlist") });
    });

    it("should ask when defaultToolBehavior is always_ask", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          defaultToolBehavior: "always_ask",
        },
      ]);
      const result = checkMcpApproval({ settings, request: makeToolCall() });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("resource read requests", () => {
    it("should approve resource matching configured pattern with alwaysAllow", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          resources: [{ uriPattern: "file:///*", alwaysAllow: true }],
        },
      ]);
      const result = checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///test.txt" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should ask for resource matching configured pattern without alwaysAllow", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          resources: [{ uriPattern: "file:///*" }],
        },
      ]);
      const result = checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///test.txt" }),
      });
      expect(result).toEqual({ decision: "ask" });
    });

    it("should approve with defaultResourceBehavior always_approve", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          defaultResourceBehavior: "always_approve",
        },
      ]);
      const result = checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///unknown.txt" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should ask with defaultResourceBehavior always_ask", () => {
      const settings = makeSettings([
        {
          name: "test-server",
          defaultResourceBehavior: "always_ask",
        },
      ]);
      const result = checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///unknown.txt" }),
      });
      expect(result).toEqual({ decision: "ask" });
    });
  });

  describe("list requests", () => {
    it("should approve list_tools requests", () => {
      const result = checkMcpApproval({
        settings: makeSettings([{ name: "test-server" }]),
        request: makeListRequest({ type: "list_tools" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });

    it("should approve list_resources requests", () => {
      const result = checkMcpApproval({
        settings: makeSettings([{ name: "test-server" }]),
        request: makeListRequest({ type: "list_resources" }),
      });
      expect(result).toEqual({ decision: "approve" });
    });
  });
});

// ============================================================================
// matchUriPattern (tested indirectly through checkMcpApproval)
// ============================================================================
describe("URI pattern matching (indirect)", () => {
  it("should match exact URI patterns", () => {
    const settings = makeSettings([
      {
        name: "test-server",
        resources: [{ uriPattern: "file:///exact.txt", alwaysAllow: true }],
      },
    ]);
    expect(
      checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///exact.txt" }),
      }),
    ).toEqual({ decision: "approve" });
    expect(
      checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///other.txt" }),
      }),
    ).not.toEqual({ decision: "approve" });
  });

  it("should match wildcard URI patterns", () => {
    const settings = makeSettings([
      {
        name: "test-server",
        resources: [{ uriPattern: "file:///data/*", alwaysAllow: true }],
      },
    ]);
    expect(
      checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///data/123.json" }),
      }),
    ).toEqual({ decision: "approve" });
    expect(
      checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///other/123.json" }),
      }),
    ).not.toEqual({ decision: "approve" });
  });

  it("should match URI patterns with ? placeholder", () => {
    const settings = makeSettings([
      {
        name: "test-server",
        resources: [{ uriPattern: "file:///file.?", alwaysAllow: true }],
      },
    ]);
    expect(
      checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///file.a" }),
      }),
    ).toEqual({ decision: "approve" });
    expect(
      checkMcpApproval({
        settings,
        request: makeResourceRead({ uri: "file:///file.ab" }),
      }),
    ).not.toEqual({ decision: "approve" });
  });
});

// ============================================================================
// createDefaultMcpApprovalSettings
// ============================================================================
describe("createDefaultMcpApprovalSettings", () => {
  it("should return empty servers array", () => {
    const settings = createDefaultMcpApprovalSettings();
    expect(settings.servers).toEqual([]);
  });

  it("should set defaultServerBehavior to always_ask", () => {
    const settings = createDefaultMcpApprovalSettings();
    expect(settings.defaultServerBehavior).toBe("always_ask");
  });
});

// ============================================================================
// mergeMcpApprovalSettings
// ============================================================================
describe("mergeMcpApprovalSettings", () => {
  it("should merge with override servers", () => {
    const base = createDefaultMcpApprovalSettings();
    const override = {
      servers: [{ name: "custom-server", tools: [{ name: "tool1", alwaysAllow: true }] }],
    };
    const merged = mergeMcpApprovalSettings(base, override);
    expect(merged.servers).toEqual(override.servers);
    expect(merged.defaultServerBehavior).toBe("always_ask");
  });

  it("should use base defaultServerBehavior when not overridden", () => {
    const base = createDefaultMcpApprovalSettings();
    const merged = mergeMcpApprovalSettings(base, {});
    expect(merged.defaultServerBehavior).toBe("always_ask");
  });

  it("should override defaultServerBehavior", () => {
    const base = createDefaultMcpApprovalSettings();
    const merged = mergeMcpApprovalSettings(base, { defaultServerBehavior: "always_deny" });
    expect(merged.defaultServerBehavior).toBe("always_deny");
  });

  it("should use override servers even when empty array", () => {
    const base: McpApprovalSettings = {
      servers: [{ name: "original", tools: [] }],
      defaultServerBehavior: "always_ask",
    };
    const merged = mergeMcpApprovalSettings(base, { servers: [] });
    expect(merged.servers).toEqual([]);
  });
});

// ============================================================================
// isServerConfigured
// ============================================================================
describe("isServerConfigured", () => {
  it("should return true when server exists", () => {
    const settings = makeSettings([{ name: "my-server" }]);
    expect(isServerConfigured(settings, "my-server")).toBe(true);
  });

  it("should return false when server does not exist", () => {
    const settings = makeSettings([{ name: "my-server" }]);
    expect(isServerConfigured(settings, "other-server")).toBe(false);
  });

  it("should return false for empty settings", () => {
    expect(isServerConfigured(makeSettings(), "anything")).toBe(false);
  });
});

// ============================================================================
// getAutoApprovedTools
// ============================================================================
describe("getAutoApprovedTools", () => {
  it("should return list of alwaysAllow tool names", () => {
    const settings = makeSettings([
      {
        name: "my-server",
        tools: [
          { name: "tool1", alwaysAllow: true },
          { name: "tool2", alwaysAllow: false },
          { name: "tool3" },
        ],
      },
    ]);
    const tools = getAutoApprovedTools(settings, "my-server");
    expect(tools).toEqual(["tool1"]);
  });

  it("should return empty array when server not found", () => {
    const tools = getAutoApprovedTools(makeSettings(), "non-existent");
    expect(tools).toEqual([]);
  });

  it("should return empty array when server has no tools", () => {
    const settings = makeSettings([{ name: "empty-server" }]);
    const tools = getAutoApprovedTools(settings, "empty-server");
    expect(tools).toEqual([]);
  });

  it("should return empty array when no tools are alwaysAllow", () => {
    const settings = makeSettings([
      {
        name: "my-server",
        tools: [
          { name: "tool1", alwaysAllow: false },
          { name: "tool2" },
        ],
      },
    ]);
    const tools = getAutoApprovedTools(settings, "my-server");
    expect(tools).toEqual([]);
  });
});