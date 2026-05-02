/**
 * Agent Loop Configuration Validator
 * Provides validation logic for AgentLoopConfigFile
 */

import type { AgentLoopConfigFile } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Validate Agent Loop configuration
 * @param config The configuration object
 * @returns Validation result
 */
export function validateAgentLoopConfig(
  config: AgentLoopConfigFile,
): Result<AgentLoopConfigFile, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (!config.id) {
    errors.push(new SchemaValidationError("id is required", { field: "id" }));
  }

  if (!config.profileId) {
    errors.push(new SchemaValidationError("profileId is required", { field: "profileId" }));
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(config);
}

/**
 * Get validation warnings for Agent Loop configuration
 * @param config The configuration object
 * @returns Array of warning messages
 */
export function getAgentLoopValidationWarnings(config: AgentLoopConfigFile): string[] {
  const warnings: string[] = [];

  if (config.maxIterations && config.maxIterations > 100) {
    warnings.push("maxIterations is unusually high (> 100). Consider reducing it to avoid excessive costs.");
  }

  return warnings;
}
