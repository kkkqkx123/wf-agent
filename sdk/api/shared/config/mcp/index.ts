/**
 * MCP Configuration Module
 * Moved from services/mcp/config to api/shared/config/mcp as part of
 * consolidating all configuration under api/shared/config.
 */

export {
  ServerConfigSchema,
  McpSettingsSchema,
  validateServerConfig,
  validateMcpSettings,
  isStdioConfig,
  isSseConfig,
  isStreamableHttpConfig,
} from "./schema.js";

export {
  DEFAULT_MCP_SETTINGS_FILE,
  PROJECT_MCP_FILE,
  loadMcpSettings,
  loadServerConfigs,
  fileExists,
  getGlobalMcpSettingsPath,
  getProjectMcpPath,
  createDefaultMcpSettings,
  writeMcpSettings,
  ensureMcpSettingsFile,
  mergeServerConfigs,
} from "./loader.js";