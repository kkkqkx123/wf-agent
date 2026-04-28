/**
 * Prompt Template Configuration Validation Function
 * Responsible for verifying the validity of prompt template configurations
 */

import type { PromptTemplateConfigFile } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import {
  validateRequiredFields,
  validateStringField,
  validateEnumField,
  validateArrayField,
} from "./base-validator.js";

/**
 * Verify prompt word template configuration
 * @param config Configuration object
 * @returns Verification result
 */
export function validatePromptTemplateConfig(
  config: PromptTemplateConfigFile,
): Result<PromptTemplateConfigFile, ValidationError[]> {
  const errors: ValidationError[] = [];

  // Verify required fields
  const requiredFields = ["id"];
  errors.push(
    ...validateRequiredFields(
      config as unknown as Record<string, unknown>,
      requiredFields,
      "prompt_template",
    ),
  );

  // Verify the id field
  if (config.id !== undefined) {
    errors.push(
      ...validateStringField(config.id, "id", {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Verify the name field (if it exists).
  if (config.name !== undefined) {
    errors.push(
      ...validateStringField(config.name, "name", {
        minLength: 1,
        maxLength: 200,
      }),
    );
  }

  // Verify the description field (if it exists).
  if (config.description !== undefined) {
    errors.push(
      ...validateStringField(config.description, "description", {
        maxLength: 1000,
      }),
    );
  }

  // Verify the category field (if it exists).
  if (config.category !== undefined) {
    errors.push(
      ...validateEnumField(config.category, "category", [
        "system",
        "rules",
        "user-command",
        "tools",
        "composite",
      ]),
    );
  }

  // Verify the content field (if it exists).
  if (config.content !== undefined) {
    errors.push(
      ...validateStringField(config.content, "content", {
        minLength: 1,
      }),
    );
  }

  // Verify the variables field (if it exists).
  if (config.variables !== undefined) {
    errors.push(...validateArrayField(config.variables, "variables"));
    // Verify each variable definition.
    if (Array.isArray(config.variables)) {
      config.variables.forEach((variable, index) => {
        errors.push(...validateVariableDefinition(variable, index));
      });
    }
  }

  // Verify the fragments field (if it exists).
  if (config.fragments !== undefined) {
    errors.push(...validateArrayField(config.fragments, "fragments"));
    // Verify each fragment ID
    if (Array.isArray(config.fragments)) {
      config.fragments.forEach((fragmentId, index) => {
        errors.push(
          ...validateStringField(fragmentId, `fragments[${index}]`, {
            minLength: 1,
          }),
        );
      });
    }
  }

  // Return the validation results.
  if (errors.length > 0) {
    return err(errors);
  }

  return ok(config);
}

/**
 * Verify variable definition
 * @param variable: Variable definition
 * @param index: Variable index
 * @returns: Array of verification errors
 */
function validateVariableDefinition(variable: unknown, index: number): ValidationError[] {
  const errors: ValidationError[] = [];
  const prefix = `variables[${index}]`;

  // Verify required fields
  const requiredFields = ["name", "type", "required"];
  const variableRecord = variable as unknown as Record<string, unknown>;
  errors.push(...validateRequiredFields(variableRecord, requiredFields, prefix));

  // Verify the name field.
  if (variableRecord["name"] !== undefined) {
    errors.push(
      ...validateStringField(variableRecord["name"] as string, `${prefix}.name`, {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Verify the type field
  if (variableRecord["type"] !== undefined) {
    errors.push(
      ...validateEnumField(variableRecord["type"] as string, `${prefix}.type`, [
        "string",
        "number",
        "boolean",
        "object",
      ]),
    );
  }

  // Verify the required fields.
  if (variableRecord["required"] !== undefined && typeof variableRecord["required"] !== "boolean") {
    errors.push(
      new SchemaValidationError(`${prefix}.required must be a boolean`, {
        field: `${prefix}.required`,
        value: variableRecord["required"],
      }),
    );
  }

  // Verify the description field (if it exists).
  if (variableRecord["description"] !== undefined) {
    errors.push(
      ...validateStringField(variableRecord["description"] as string, `${prefix}.description`, {
        maxLength: 500,
      }),
    );
  }

  return errors;
}
