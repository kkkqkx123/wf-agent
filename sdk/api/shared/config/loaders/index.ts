/**
 * Loaders module export
 * Provides configuration loading functions with file I/O operations
 */

// File loader (generic)
export {
  readConfigFile,
  getConfigFormatFromPath,
  loadConfigFile,
  tryLoadConfigFile,
  // Backward compatibility aliases
  getConfigFormatFromPath as detectConfigFormat,
  loadConfigFile as loadConfigContent,
} from "./config-file-loader.js";

// Config loader factory
export {
  createConfigFileLoader,
} from "./config-loader-factory.js";

// Prompt template loader
export {
  loadPromptTemplateConfig,
  mergePromptTemplateConfig,
  loadAndMergePromptTemplate,
} from "./prompt-template-config-loader.js";

// Metrics configuration loader
export {
  loadMetricsConfigFromFile,
} from "./metrics-config-loader.js";

// Timeout configuration loader
export {
  loadTimeoutConfigFromFile,
} from "./timeout-config-loader.js";

// File checkpoint configuration loader
export {
  loadFileCheckpointConfigFromFile,
} from "./file-checkpoint-config-loader.js";

// MCP configuration loader
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
