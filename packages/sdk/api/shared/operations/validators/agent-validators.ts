/**
 * Agent Command Validators
 * Validation rules specific to agent commands
 */

import { validateRequiredEntity, validateRequiredId, validateOptionalPositiveInt, combineErrors } from "../validation-utils.js";
import type { CommandValidationResult } from "../../types/command.js";
import { validationSuccess, validationFailure } from "../../types/command.js";

/**
 * Validate agent loop run parameters
 */
export function validateAgentLoopRunParams(config: unknown): CommandValidationResult {
  const c = config as Record<string, unknown>;
  const errors = combineErrors(
    validateRequiredEntity(config, "Config"),
    validateOptionalPositiveInt(c?.['maxIterations'] as number | undefined, "maxIterations")
  );

  // Validate profileId if provided (must be non-empty string)
  if (
    c?.['profileId'] !== undefined &&
    typeof c['profileId'] === "string" &&
    c['profileId'].trim().length === 0
  ) {
    errors.push("`profileId` cannot be an empty string.");
  }

  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate agent loop control parameters (pause, resume, cancel)
 * Standardized error messages for agent loop lifecycle operations
 */
export function validateAgentLoopControlParams(agentLoopId: string): CommandValidationResult {
  const errors = validateRequiredId(agentLoopId, "Agent Loop ID");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate agent checkpoint creation parameters
 */
export function validateAgentCheckpointCreationParams(agentLoopId: string): CommandValidationResult {
  const errors = validateRequiredId(agentLoopId, "Agent Loop ID");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate agent checkpoint restoration parameters
 */
export function validateAgentCheckpointRestorationParams(checkpointId: string): CommandValidationResult {
  const errors = validateRequiredId(checkpointId, "Checkpoint ID");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate agent trigger parameters (enable/disable)
 */
export function validateAgentTriggerParams(agentLoopId: string, triggerId: string): CommandValidationResult {
  const errors: string[] = [];
  if (!agentLoopId) {
    errors.push("Agent Loop ID is required");
  }
  if (!triggerId) {
    errors.push("Trigger ID is required");
  }
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}
