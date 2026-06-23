/**
 * Shared Command Validators
 * Common validation rules for multiple command types
 */

import { validateRequiredString, validateRequiredId } from "../validation-utils.js";
import type { CommandValidationResult } from "../../types/command.js";
import { validationSuccess, validationFailure } from "../../types/command.js";

/**
 * Validate LLM generate request parameters
 */
export function validateGenerateParams(request: any): CommandValidationResult {
  const errors: string[] = [];

  if (!request) {
    errors.push("LLM request must be provided.");
  } else if (!request.messages || request.messages.length === 0) {
    errors.push("The message list cannot be empty.");
  }

  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate tool execution parameters
 */
export function validateToolExecutionParams(toolId: string, parameters: any): CommandValidationResult {
  const errors: string[] = [];

  errors.push(...validateRequiredId(toolId, "Tool ID"));

  if (!parameters) {
    errors.push("The parameter cannot be null.");
  }

  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate script execution parameters
 */
export function validateScriptExecutionParams(scriptName: string): CommandValidationResult {
  const errors = validateRequiredString(scriptName, "Script name");
  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}

/**
 * Validate event dispatch parameters
 */
export function validateEventDispatchParams(event: any): CommandValidationResult {
  const errors: string[] = [];

  if (!event) {
    errors.push("Event object cannot be empty");
  } else if (!event.type) {
    errors.push("Event type cannot be empty");
  }

  return errors.length > 0 ? validationFailure(errors) : validationSuccess();
}
