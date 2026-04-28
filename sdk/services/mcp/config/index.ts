/**
 * MCP Configuration Module
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
