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
export function validateAgentLoopRunParams(config: any): CommandValidationResult {
  const errors = combineErrors(
    validateRequiredEntity(config, "Config"),
    validateOptionalPositiveInt(config?.maxIterations, "maxIterations")
  );

  // Validate profileId if provided (must be non-empty string)
  if (
    config?.profileId !== undefined &&
    typeof config.profileId === "string" &&
    config.profileId.trim().length === 0
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
