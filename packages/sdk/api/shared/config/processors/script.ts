/**
 * Script Configuration Processing Functions
 * Provide functions for validating, converting, and exporting Script configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { CodeConfigValidator } from "../../../../workflow/validation/script-config-validator.js";
import { ok, err } from "@wf-agent/common-utils";
import type { Script } from "@wf-agent/types";
import { substituteParameters } from "../config-utils.js";
import { ConfigFormat } from "../types.js";
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";

/**
 * Parse script configuration from raw content.
 *
 * @param content - Raw file content (TOML or JSON string).
 * @param format - Expected format.
 * @returns The parsed Script.
 */
export function parseScript(content: string, format: ConfigFormat): Script {
  const raw: unknown = format === "toml" ? parseToml(content) : parseJson(content);
  return raw as Script;
}

/**
 * Verify Script Configuration
 * Uses CodeConfigValidator for validation
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateScript(
  config: ParsedConfig<"script">,
): Result<ParsedConfig<"script">, ValidationError[]> {
  const script = config.config as Script;

  // Use CodeConfigValidator for validation
  const validator = new CodeConfigValidator();
  const result = validator.validateScript(script);

  if (result.isErr()) {
    return err(result.error);
  }

  return ok(config);
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
  // substituteParameters is idempotent for empty parameters
  return substituteParameters(config.config, parameters);
}

/**
 * Export Script Configuration
 * Returns typed data ready for serialization.
 * @param script Script object
 * @returns The script data ready for export
 */
export function exportScript(script: Script): Script {
  return script;
}
