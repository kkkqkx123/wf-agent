/**
 * MCP Configuration Module
 *
 * Re-exports MCP connection processing functions from the canonical
 * location within the MCP service layer.
 */

export {
  loadServerConfigs,
  createDefaultMcpSettings,
  mergeServerConfigs,
  resolveServerLifecycle,
} from "../mcp-connection-processor.js";
export type { ResolvedLifecycle } from "../mcp-connection-processor.js";
