/**
 * MCP Service Module
 * Main entry point for MCP server management
 */

// Types
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
} from "./types.js";

// Config
export {
  loadServerConfigs,
  createDefaultMcpSettings,
  mergeServerConfigs,
  resolveServerLifecycle,
} from "./config/index.js";
export type { ResolvedLifecycle } from "./config/index.js";

// Transport
export {
  type IMcpTransport,
  type TransportConfig,
  type TransportEventHandlers,
  type TransportOptions,
  StdioTransport,
  SseTransport,
  StreamableHttpTransport,
  createTransport,
  isTransportTypeSupported,
} from "./transport/index.js";

// Client
export { McpClient } from "./mcp-client.js";

// Connection State
export {
  createInitialServerState,
  updateServerStatus,
  addErrorToHistory,
  clearErrorState,
  isConnectable,
  isConnected,
  isDisabled,
  getServerDisplayName,
  updateLastActivity,
  updateLastHealthCheck,
  isIdleBeyond,
} from "./connection-state.js";

// Connection Manager
export { McpConnectionManager } from "./connection-manager.js";

// Server Registry
export { McpServerRegistry, getMcpManager, releaseMcpManager } from "./server-registry.js";
