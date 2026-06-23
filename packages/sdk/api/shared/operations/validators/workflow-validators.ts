/**
 * Workflow Command Validators
 * Validation rules specific to workflow commands
 */

import { validateRequiredId, validateRequiredString } from "../validation-utils.js";
import type { CommandValidationResult } from "../../types/command.js";
import { validationSuccess, validationFailure } from "../../types/command.js";

/**
 * Validate workflow execution parameters
 */
export function validateWorkflowExecutionParams(workflowId: string): CommandValidationResult {
  const errors = validateRequiredId(workflowId, "Workflow ID");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate workflow lifecycle parameters (pause, resume, cancel)
 */
export function validateWorkflowLifecycleParams(executionId: string): CommandValidationResult {
  const errors = validateRequiredId(executionId, "Execution ID");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Checkpoint creation parameters validation
 * Uses validateRequiredId since executionId is an identifier, not arbitrary text
 */
export function validateCheckpointCreationParams(executionId: string): CommandValidationResult {
  const errors = validateRequiredId(executionId, "Execution ID");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate checkpoint restoration parameters
 */
export function validateCheckpointRestorationParams(checkpointId: string): CommandValidationResult {
  const errors = validateRequiredString(checkpointId, "Checkpoint ID");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate trigger parameters
 * Standardized error messages for trigger enable/disable operations
 */
export function validateTriggerParams(executionId: string, triggerId: string): CommandValidationResult {
  const errors: string[] = [];

  errors.push(...validateRequiredId(executionId, "Execution ID"));
  errors.push(...validateRequiredId(triggerId, "Trigger ID"));

  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}
