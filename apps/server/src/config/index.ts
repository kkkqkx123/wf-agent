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
} from "./cli/types.js";

// Re-export configuration types
export type {
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
} from "@wf-agent/types";

// Presets types moved to @wf-agent/sdk/resources
export type {
  ContextCompressionPresetConfig,
  PredefinedToolsPresetConfig,
  PredefinedPromptsPresetConfig,
  PresetsConfig,
} from "@wf-agent/sdk/resources";

// Validator
export { ConfigValidator } from "./config-validator.js";
