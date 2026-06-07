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
import { substituteParameters } from "../utils/config-utils.js";

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
  let script = config.config;

  // If parameters are provided, perform parameter substitution
  if (parameters && Object.keys(parameters).length > 0) {
    script = substituteParameters(script, parameters);
  }

  return script;
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
