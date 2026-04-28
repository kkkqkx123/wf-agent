/**
 * MCP Approval Checker
 * Implements fine-grained MCP tool and resource approval
 */

import type {
  McpApprovalSettings,
  McpServerConfig,
  McpToolConfig,
  McpRequest,
  McpToolCallRequest,
  McpResourceReadRequest,
  McpApprovalDecision,
} from "@wf-agent/types";
import { isMcpToolCallRequest, isMcpResourceReadRequest, isMcpListRequest } from "@wf-agent/types";

/**
 * Check if an MCP request should be auto-approved
 *
 * @param params - Check parameters
 * @returns Approval decision
 */
export function checkMcpApproval(params: {
  settings: McpApprovalSettings;
  request: McpRequest;
}): McpApprovalDecision {
  const { settings, request } = params;

  // 1. Find server configuration
  const serverConfig = settings.servers.find(s => s.name === request.serverName);

  if (!serverConfig) {
    // Unknown server - use default behavior
    switch (settings.defaultServerBehavior ?? "always_ask") {
      case "always_ask":
        return { decision: "ask" };
      case "always_deny":
        return { decision: "deny", reason: `Unknown MCP server: ${request.serverName}` };
    }
  }

  // 2. Handle by request type
  if (isMcpToolCallRequest(request)) {
    return checkMcpToolApproval(serverConfig!, request);
  }

  if (isMcpResourceReadRequest(request)) {
    return checkMcpResourceApproval(serverConfig!, request);
  }

  if (isMcpListRequest(request)) {
    // List operations are generally safe
    return { decision: "approve" };
  }

  // Unknown request type
  return { decision: "ask" };
}

/**
 * Check MCP tool call approval
 *
 * @param serverConfig - Server configuration
 * @param request - Tool call request
 * @returns Approval decision
 */
function checkMcpToolApproval(
  serverConfig: McpServerConfig,
  request: McpToolCallRequest,
): McpApprovalDecision {
  // 1. Find tool configuration
  const toolConfig = serverConfig.tools?.find(t => t.name === request.toolName);

  if (toolConfig) {
    // Tool explicitly configured
    if (toolConfig.alwaysAllow) {
      return { decision: "approve" };
    }

    // Check risk level override
    if (toolConfig.riskLevel === "READ_ONLY") {
      return { decision: "approve" };
    }

    return { decision: "ask" };
  }

  // 2. Tool not explicitly configured - use default behavior
  switch (serverConfig.defaultToolBehavior ?? "always_ask") {
    case "always_approve":
      return { decision: "approve" };
    case "always_deny":
      return { decision: "deny", reason: `Tool ${request.toolName} not in allowlist` };
    default:
      return { decision: "ask" };
  }
}

/**
 * Check MCP resource read approval
 *
 * @param serverConfig - Server configuration
 * @param request - Resource read request
 * @returns Approval decision
 */
function checkMcpResourceApproval(
  serverConfig: McpServerConfig,
  request: McpResourceReadRequest,
): McpApprovalDecision {
  // 1. Find matching resource configuration
  const resourceConfig = serverConfig.resources?.find(r =>
    matchUriPattern(request.uri, r.uriPattern),
  );

  if (resourceConfig) {
    if (resourceConfig.alwaysAllow) {
      return { decision: "approve" };
    }
    return { decision: "ask" };
  }

  // 2. Resource not explicitly configured
  switch (serverConfig.defaultResourceBehavior ?? "always_ask") {
    case "always_approve":
      return { decision: "approve" };
    default:
      return { decision: "ask" };
  }
}

/**
 * Match URI against pattern
 * Supports simple wildcard patterns like "file:///*"
 *
 * @param uri - The URI to match
 * @param pattern - The pattern to match against
 * @returns True if the URI matches the pattern
 */
function matchUriPattern(uri: string, pattern: string): boolean {
  // Convert glob pattern to regex
  const regexPattern = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&") // Escape special regex chars
    .replace(/\*/g, ".*") // Convert * to .*
    .replace(/\?/g, "."); // Convert ? to .

  const regex = new RegExp(`^${regexPattern}$`);
  return regex.test(uri);
}

/**
 * Create default MCP approval settings
 *
 * @returns Default MCP approval settings
 */
export function createDefaultMcpApprovalSettings(): McpApprovalSettings {
  return {
    servers: [],
    defaultServerBehavior: "always_ask",
  };
}

/**
 * Merge MCP approval settings
 * Allows combining multiple settings with precedence
 *
 * @param base - Base settings
 * @param override - Override settings
 * @returns Merged settings
 */
export function mergeMcpApprovalSettings(
  base: McpApprovalSettings,
  override: Partial<McpApprovalSettings>,
): McpApprovalSettings {
  return {
    servers: override.servers ?? base.servers,
    defaultServerBehavior: override.defaultServerBehavior ?? base.defaultServerBehavior,
  };
}

/**
 * Check if a server is configured
 *
 * @param settings - MCP approval settings
 * @param serverName - Server name
 * @returns True if the server is configured
 */
export function isServerConfigured(settings: McpApprovalSettings, serverName: string): boolean {
  return settings.servers.some(s => s.name === serverName);
}

/**
 * Get all auto-approved tools for a server
 *
 * @param settings - MCP approval settings
 * @param serverName - Server name
 * @returns List of auto-approved tool names
 */
export function getAutoApprovedTools(settings: McpApprovalSettings, serverName: string): string[] {
  const serverConfig = settings.servers.find(s => s.name === serverName);
  if (!serverConfig?.tools) {
    return [];
  }

  return serverConfig.tools.filter(t => t.alwaysAllow).map(t => t.name);
}
