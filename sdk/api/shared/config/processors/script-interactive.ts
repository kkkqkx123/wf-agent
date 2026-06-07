/**
 * Interactive Script Configuration Processing Functions
 * Provide functions for validating, converting, and exporting InteractiveScriptConfig.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { InteractiveScriptConfig } from "@wf-agent/types";
import { substituteParameters } from "../utils/config-utils.js";

/**
 * Validate InteractiveScriptConfig configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateInteractiveScript(
  config: ParsedConfig<"script">,
): Result<ParsedConfig<"script">, ValidationError[]> {
  const interactiveConfig = config.config as unknown as InteractiveScriptConfig;

  const errors: ValidationError[] = [];

  const validModes = ["blocking", "llm-assisted", "hybrid"];
  if (interactiveConfig.mode && !validModes.includes(interactiveConfig.mode)) {
    errors.push(
      new ValidationError(
        `Invalid interaction mode '${interactiveConfig.mode}'. Must be one of: ${validModes.join(", ")}`,
        "mode",
      ),
    );
  }

  if (interactiveConfig.interactionPoints) {
    if (!Array.isArray(interactiveConfig.interactionPoints)) {
      errors.push(
        new ValidationError("interactionPoints must be an array", "interactionPoints"),
      );
    } else {
      for (const point of interactiveConfig.interactionPoints) {
        if (!point.prompt) {
          errors.push(
            new ValidationError("Each interaction point must have a prompt", "interactionPoints.prompt"),
          );
        }
      }
    }
  }

  if (interactiveConfig.maxRounds !== undefined && (typeof interactiveConfig.maxRounds !== "number" || interactiveConfig.maxRounds < 1)) {
    errors.push(
      new ValidationError("maxRounds must be a positive number", "maxRounds"),
    );
  }

  if (interactiveConfig.roundTimeout !== undefined && (typeof interactiveConfig.roundTimeout !== "number" || interactiveConfig.roundTimeout < 0)) {
    errors.push(
      new ValidationError("roundTimeout must be a non-negative number", "roundTimeout"),
    );
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(config);
}

/**
 * Transform InteractiveScriptConfig
 * Handle parameter substitution
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The converted InteractiveScriptConfig
 */
export function transformInteractiveScript(
  config: ParsedConfig<"script">,
  parameters?: Record<string, unknown>,
): InteractiveScriptConfig {
  let interactiveConfig = config.config as unknown as InteractiveScriptConfig;

  if (parameters && Object.keys(parameters).length > 0) {
    interactiveConfig = substituteParameters(interactiveConfig, parameters) as unknown as InteractiveScriptConfig;
  }

  return interactiveConfig;
}

/**
 * Export InteractiveScriptConfig
 * Returns typed data ready for serialization.
 * @param config InteractiveScriptConfig object
 * @returns The interactive script config ready for export
 */
export function exportInteractiveScript(config: InteractiveScriptConfig): InteractiveScriptConfig {
  return config;
}