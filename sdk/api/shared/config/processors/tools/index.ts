/**
 * Tools Configuration Processors
 *
 * Provides validation, transformation, and export for tool-specific configurations.
 * These processors are used when loading tool configs from configuration files
 * or when merging runtime defaults.
 *
 * Currently supports:
 * - ReadFileConfig: File reading tool configuration
 */

export {
  validateReadFileConfig,
  transformReadFileConfig,
  exportReadFileConfig,
  type ReadFileConfigInput,
} from "./read-file.js";