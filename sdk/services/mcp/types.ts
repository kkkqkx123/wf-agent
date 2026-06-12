/**
 * MCP Service Type Definitions
 *
 * Re-exports connection-level MCP types from @wf-agent/types
 * for backward compatibility within the services layer.
 */

export type {
  McpServerStatus,
  McpTransportType,
  McpServerSource,
  McpServerLifecycle,
  McpServerConfigBase,
  McpStdioConfig,
  McpSseConfig,
  McpStreamableHttpConfig,
  McpServerConfig,
  McpTool,
  McpResource,
  McpResourceTemplate,
  McpErrorEntry,
  McpServerState,
  McpToolCallResult,
  McpResourceReadResult,
  McpSettings,
  McpConnectionState,
  McpManagerOptions,
  McpEventType,
  McpEventHandler,
} from "@wf-agent/types";
