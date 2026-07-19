/**
 * CLI Configuration Validator
 * Extends the base ConfigValidator from @wf-agent/runtime with
 * CLI-specific validation rules.
 */

import type { CLIConfig } from "./cli/types.js";
import { getOutput } from "../utils/output.js";

const output = getOutput();

/**
 * CLI Configuration Validator
 * Provides CLI-specific configuration validation.
 * Extends the base ConfigValidator from runtime.
 */
export class ConfigValidator {
  /**
   * Validate CLI configuration.
   * @param config Configuration object to validate
   * @returns Validation result with errors if any
   */
  static validate(config: CLIConfig): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    if (config.storage) {
      if (config.storage.type === "sqlite") {
        if (!config.storage.sqlite) {
          errors.push("SQLite storage configuration is required when storage type is 'sqlite'");
        }
      }
    }

    if (config.output) {
      if (config.output.sdkLogLevel === "silent" && config.output.enableSDKLogs) {
        output.warnLog("SDK logs are enabled but log level is 'silent', no logs will be output");
      }
    }

    if (config.presets) {
      if (config.presets.predefinedTools) {
        const { allowList, blockList } = config.presets.predefinedTools;
        if (allowList && blockList) {
          const intersection = allowList.filter(item => blockList.includes(item));
          if (intersection.length > 0) {
            errors.push(
              `Predefined tools allowList and blockList have intersection: ${intersection.join(", ")}`,
            );
          }
        }
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Validate configuration and throw error if invalid.
   * @param config Configuration object to validate
   * @throws Error if configuration is invalid
   */
  static validateOrThrow(config: CLIConfig): void {
    const { valid, errors } = this.validate(config);
    if (!valid) {
      throw new Error(`Configuration validation failed:\n${errors.join("\n")}`);
    }
  }
}
