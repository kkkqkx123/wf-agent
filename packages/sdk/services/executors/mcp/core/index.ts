/**
 * MCP Core Module
 * Core connection management and lifecycle
 */

export { McpConnectionManager } from "./connection-manager.js";
export { McpClient } from "./mcp-client.js";
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

export { McpServerRegistry, getMcpManager, releaseMcpManager } from "./server-registry.js";
