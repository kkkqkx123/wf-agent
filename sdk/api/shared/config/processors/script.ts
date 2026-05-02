/**
 * Script Configuration Processing Functions
 * Provide functions for validating, converting, and exporting Script configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { CodeConfigValidator } from "../../../../workflow/validation/script-config-validator.js";
import {
  validateRequiredFields,
  validateBooleanField,
} from "../validators/validation-helpers.js";
import { ok, err } from "@wf-agent/common-utils";
import type { Script } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";
import { substituteParameters } from "../config-utils.js";

/**
 * Verify Script Configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateScript(
  config: ParsedConfig<"script">,
): Result<ParsedConfig<"script">, ValidationError[]> {
  const script = config.config as Script;
  const errors: ValidationError[] = [];
  const codeConfigValidator = new CodeConfigValidator();

  // Verify required fields
  errors.push(
    ...validateRequiredFields(
      script as unknown as Record<string, unknown>,
      ["id", "name", "type", "description", "options"],
      "Script",
    ),
  );

  // Verify the activation status.
  if (script.enabled !== undefined) {
    errors.push(...validateBooleanField(script.enabled, "Script.enabled"));
  }

  // Fully delegate the script configuration validation to CodeConfigValidator.
  const scriptResult = codeConfigValidator.validateScript(script);
  if (scriptResult.isErr()) {
    errors.push(...scriptResult.error);
  }

  // Verify script type compatibility
  if (script.type && (script.content || script.filePath)) {
    const compatibilityResult = codeConfigValidator.validateScriptTypeCompatibility(
      script.type,
      script.content,
      script.filePath,
    );
    if (compatibilityResult.isErr()) {
      errors.push(...compatibilityResult.error);
    }
  }

  if (errors.length > 0) {
    return err(errors) as Result<ParsedConfig<"script">, ValidationError[]>;
  }

  return ok(config) as Result<ParsedConfig<"script">, ValidationError[]>;
}

/**
 * Translate Script configuration
 * Handle parameter substitution (if applicable)
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The converted Script
 */
export function transformScript(
  config: ParsedConfig<"script">,
  parameters?: Record<string, unknown>,
): Script {
  let script = config.config;

  // If parameters are provided, perform parameter substitution
  if (parameters && Object.keys(parameters).length > 0) {
    script = substituteParameters(script, parameters);
  }

  return script;
}

/**
 * Export Script Configuration
 * @param script Script object
 * @param format Configuration format
 * @returns String containing the configuration file content
 */
export function exportScript(script: Script, format: ConfigFormat): string {
  switch (format) {
    case "json":
      return stringifyJson(script, true);
    case "toml":
      throw new ConfigurationError(
        "The TOML format does not support export; please use the JSON format.",
        format,
        {
          suggestion: "Use JSON instead.",
        },
      );
    default:
      throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
  }
}
