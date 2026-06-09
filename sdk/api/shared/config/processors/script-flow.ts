/**
 * Script Flow Configuration Processing Functions
 * Provide functions for validating, converting, and exporting ScriptFlow configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { ScriptFlow } from "@wf-agent/types";
import { substituteParameters } from "../config-utils.js";

/**
 * Validate ScriptFlow configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateScriptFlow(
  config: ParsedConfig<"script">,
): Result<ParsedConfig<"script">, ValidationError[]> {
  const scriptFlow = config.config as unknown as ScriptFlow;

  const errors: ValidationError[] = [];

  if (!scriptFlow.name) {
    errors.push(
      new ValidationError("Script flow name is required", "name"),
    );
  }

  if (!scriptFlow.branches || !Array.isArray(scriptFlow.branches)) {
    errors.push(
      new ValidationError("Script flow must have at least one branch", "branches"),
    );
  } else {
    for (const branch of scriptFlow.branches) {
      if (!branch.key) {
        errors.push(
          new ValidationError("Each branch must have a key", "branches.key"),
        );
      }
      if (!branch.modules || !Array.isArray(branch.modules) || branch.modules.length === 0) {
        errors.push(
          new ValidationError(`Branch '${branch.key}' must have at least one module`, `branches.${branch.key}.modules`),
        );
      }
    }
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(config);
}

/**
 * Transform ScriptFlow configuration
 * Handle parameter substitution
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The converted ScriptFlow
 */
export function transformScriptFlow(
  config: ParsedConfig<"script">,
  parameters?: Record<string, unknown>,
): ScriptFlow {
  // substituteParameters is idempotent for empty parameters
  return substituteParameters(config.config, parameters) as unknown as ScriptFlow;
}

/**
 * Export ScriptFlow configuration
 * Returns typed data ready for serialization.
 * @param scriptFlow ScriptFlow object
 * @returns The script flow data ready for export
 */
export function exportScriptFlow(scriptFlow: ScriptFlow): ScriptFlow {
  return scriptFlow;
}