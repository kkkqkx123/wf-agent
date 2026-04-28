/**
 * Configuration Module Exports
 * Unified exports for all configuration modules.
 */

// CLI Configuration
export {
  loadConfig,
  loadConfigWithEnvOverride,
} from "./cli/loader.js";

export {
  CLIConfigAccessor,
  getCLIConfigAccessor,
  initCLIConfigAccessor,
  resetCLIConfigAccessor,
} from "./cli/accessor.js";

export { DEFAULT_CONFIG } from "./cli/defaults.js";

export { CLIConfigSchema } from "./cli/schema.js";

export type {
  CLIConfig,
  LogLevel,
  OutputFormat,
  SDKLogLevel,
  StorageType,
  CompressionAlgorithm,
  CompressionConfig,
  JsonStorageConfig,
  SqliteStorageConfig,
  StorageConfig,
  OutputConfig,
  ContextCompressionPresetConfig,
  PredefinedToolsPresetConfig,
  PredefinedPromptsPresetConfig,
  PresetsConfig,
} from "./cli/types.js";

// Validator
export { ConfigValidator } from "./config-validator.js";
