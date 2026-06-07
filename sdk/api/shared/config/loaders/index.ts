/**
 * Loaders module export
 *
 * Only the MCP-specific loader remains in the SDK because it involves
 * bidirectional file I/O (read & write) that is tightly coupled to the
 * MCP protocol handling.
 *
 * Generic file I/O utilities (readConfigFile, loadConfigFile, …) have been
 * moved to apps/config-processor — the application layer.
 */

// MCP-specific loader (bidirectional file I/O — read & write at runtime)
export {
  DEFAULT_MCP_SETTINGS_FILE,
  PROJECT_MCP_FILE,
  loadMcpSettings,
  fileExists,
  getGlobalMcpSettingsPath,
  getProjectMcpPath,
  writeMcpSettings,
  ensureMcpSettingsFile,
} from "./mcp-config-loader.js";