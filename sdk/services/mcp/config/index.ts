/**
 * MCP Configuration Module
 *
 * Re-exports from api/shared/config — the canonical location for all config.
 * This file exists for backward compatibility within the services layer.
 *
 * Legacy namespace split: config exports were previously re-exported via
 * services/mcp/config/index.ts. They now live directly under
 * api/shared/config and are available via processors/ and loaders/.
 */

export {
  ServerConfigSchema,
  McpSettingsSchema,
  validateServerConfig,
  validateMcpSettings,
  isStdioConfig,
  isSseConfig,
  isStreamableHttpConfig,
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
} from "../../../api/shared/config/index.js";