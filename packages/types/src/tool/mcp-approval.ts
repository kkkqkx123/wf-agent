/**
 * MCP Approval Type Definitions
 * Used for fine-grained MCP tool and resource access control
 */

import type { ToolRiskLevel } from "./risk-level.js";

/**
 * MCP Tool Configuration
 * Defines settings for a specific MCP tool
 */
export interface McpToolConfig {
  /** Tool name */
  name: string;
  /** Tool description */
  description?: string;
  /** Whether this tool is always allowed (auto-approved) */
  alwaysAllow?: boolean;
  /** Risk level override for this tool */
  riskLevel?: ToolRiskLevel;
}

/**
 * MCP Resource Configuration
 * Defines settings for a specific MCP resource
 */
export interface McpResourceConfig {
  /** Resource URI pattern (glob or exact) */
  uriPattern: string;
  /** Whether this resource is always allowed to read */
  alwaysAllow?: boolean;
  /** Resource description */
  description?: string;
}

/**
 * Default behavior for unconfigured items
 */
export type McpDefaultBehavior = "always_approve" | "always_ask" | "always_deny";

/**
 * MCP Server Configuration
 * Defines settings for a specific MCP server
 */
export interface McpServerConfig {
  /** Server name */
  name: string;
  /** Server description */
  description?: string;
  /** Tools provided by this server */
  tools?: McpToolConfig[];
  /** Resources provided by this server */
  resources?: McpResourceConfig[];
  /** Default behavior for tools not explicitly configured */
  defaultToolBehavior?: McpDefaultBehavior;
  /** Default behavior for resources not explicitly configured */
  defaultResourceBehavior?: McpDefaultBehavior;
}

/**
 * MCP Approval Settings
 * Top-level configuration for MCP approval
 */
export interface McpApprovalSettings {
  /** Server configurations */
  servers: McpServerConfig[];
  /** Global default behavior for unknown servers */
  defaultServerBehavior?: "always_ask" | "always_deny";
}

/**
 * MCP Tool Call Request
 */
export interface McpToolCallRequest {
  /** Request type */
  type: "use_mcp";
  /** Server name */
  serverName: string;
  /** Tool name */
  toolName: string;
  /** Tool arguments */
  arguments?: Record<string, unknown>;
}

/**
 * MCP Resource Read Request
 */
export interface McpResourceReadRequest {
  /** Request type */
  type: "read_resource";
  /** Server name */
  serverName: string;
  /** Resource URI */
  uri: string;
}

/**
 * MCP List Request
 */
export interface McpListRequest {
  /** Request type */
  type: "list_tools" | "list_resources";
  /** Server name */
  serverName: string;
}

/**
 * Unified MCP Request
 */
export type McpRequest = McpToolCallRequest | McpResourceReadRequest | McpListRequest;

/**
 * MCP Approval Decision
 */
export type McpApprovalDecision =
  | { decision: "approve" }
  | { decision: "deny"; reason: string }
  | { decision: "ask" };

/**
 * Create a tool call request
 */
export function createMcpToolCallRequest(
  serverName: string,
  toolName: string,
  args?: Record<string, unknown>
): McpToolCallRequest {
  return {
    type: "use_mcp",
    serverName,
    toolName,
    arguments: args,
  };
}

/**
 * Create a resource read request
 */
export function createMcpResourceReadRequest(
  serverName: string,
  uri: string
): McpResourceReadRequest {
  return {
    type: "read_resource",
    serverName,
    uri,
  };
}

/**
 * Check if request is a tool call
 */
export function isMcpToolCallRequest(request: McpRequest): request is McpToolCallRequest {
  return request.type === "use_mcp";
}

/**
 * Check if request is a resource read
 */
export function isMcpResourceReadRequest(request: McpRequest): request is McpResourceReadRequest {
  return request.type === "read_resource";
}

/**
 * Check if request is a list operation
 */
export function isMcpListRequest(request: McpRequest): request is McpListRequest {
  return request.type === "list_tools" || request.type === "list_resources";
}
