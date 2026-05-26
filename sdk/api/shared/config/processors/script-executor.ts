/**
 * Script Executor Configuration Processing Functions
 *
 * Provides functions for validating, transforming, and exporting
 * standalone executor configurations (runtime settings, shell config, etc.).
 *
 * This enables users to define reusable executor configs (e.g., docker, ssh)
 * in separate config files and reference them by name in Script definitions.
 *
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { ScriptExecutorConfigSchema } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { ScriptExecutorConfig } from "@wf-agent/types";
import { stringifyJson } from "../parsers/json-parser.js";

/**
 * Validate executor configuration.
 *
 * Validates against ScriptExecutorConfigSchema, which includes refine rules:
 *   - runtime "docker" | "ssh" → runtimeConfig is required
 *   - runtime "native" | "wsl" → runtimeConfig is not allowed
 *
 * @param config The parsed configuration object
 * @returns The validation result
 */
export function validateExecutorConfig(
  config: ParsedConfig<"executor">,
): Result<ParsedConfig<"executor">, ValidationError[]> {
  const executorConfig = config.config as ScriptExecutorConfig;
  const result = ScriptExecutorConfigSchema.safeParse(executorConfig);

  if (!result.success) {
    const errors = result.error.issues.map(
      issue =>
        new ValidationError(
          issue.message,
          issue.path.join("."),
        ),
    );
    return err(errors);
  }

  return ok(config);
}

/**
 * Transform executor configuration.
 *
 * Currently applies parameter substitution; future extensions may include
 * default value injection (e.g., default runtime to "native").
 *
 * @param config The parsed configuration object
 * @returns The transformed ScriptExecutorConfig
 */
export function transformExecutorConfig(
  config: ParsedConfig<"executor">,
): ScriptExecutorConfig {
  return { ...config.config };
}

/**
 * Export executor configuration to string.
 *
 * @param config ScriptExecutorConfig object
 * @param format Configuration format (only JSON supported for executor config)
 * @returns String containing the configuration file content
 */
export function exportExecutorConfig(
  config: ScriptExecutorConfig,
  format: ConfigFormat,
): string {
  switch (format) {
    case "json":
      return stringifyJson(config, true);
    case "toml":
      throw new ConfigurationError(
        "TOML format does not support executor config export; please use JSON format.",
        format,
        { suggestion: "Use JSON instead." },
      );
    default:
      throw new ConfigurationError(
        `Unsupported configuration format: ${format}`,
        format,
      );
  }
}