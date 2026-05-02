/**
 * Prompt Template Configuration Validator
 * Provides validation logic for PromptTemplate configurations
 */

import type { Result } from "@wf-agent/types";
import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { PromptTemplateConfigFile } from "../types.js";
import {
  validateRequiredFields,
  validateStringField,
  validateEnumField,
  validateArrayField,
} from "./validation-helpers.js";

/**
 * Validate Prompt Template configuration
 * @param cfg The PromptTemplateConfigFile object to validate
 * @returns Validation result
 */
export function validatePromptTemplateConfig(
  cfg: PromptTemplateConfigFile,
): Result<PromptTemplateConfigFile, ValidationError[]> {
  const errors: ValidationError[] = [];

  // Validate required fields
  const requiredFields = ["id"];
  errors.push(
    ...validateRequiredFields(
      cfg as unknown as Record<string, unknown>,
      requiredFields,
      "prompt_template",
    ),
  );

  // Validate id field
  if (cfg.id !== undefined) {
    errors.push(
      ...validateStringField(cfg.id, "id", {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Validate name field (if exists)
  if (cfg.name !== undefined) {
    errors.push(
      ...validateStringField(cfg.name, "name", {
        minLength: 1,
        maxLength: 200,
      }),
    );
  }

  // Validate description field (if exists)
  if (cfg.description !== undefined) {
    errors.push(
      ...validateStringField(cfg.description, "description", {
        maxLength: 1000,
      }),
    );
  }

  // Validate category field (if exists)
  if (cfg.category !== undefined) {
    errors.push(
      ...validateEnumField(cfg.category, "category", [
        "system",
        "rules",
        "user-command",
        "tools",
        "composite",
      ]),
    );
  }

  // Validate content field (if exists)
  if (cfg.content !== undefined) {
    errors.push(
      ...validateStringField(cfg.content, "content", {
        minLength: 1,
      }),
    );
  }

  // Validate variables field (if exists)
  if (cfg.variables !== undefined) {
    errors.push(...validateArrayField(cfg.variables, "variables"));
    // Validate each variable definition
    if (Array.isArray(cfg.variables)) {
      cfg.variables.forEach((variable: any, index: number) => {
        errors.push(...validateVariableDefinition(variable, index));
      });
    }
  }

  // Validate fragments field (if exists)
  if (cfg.fragments !== undefined) {
    errors.push(...validateArrayField(cfg.fragments, "fragments"));
    // Validate each fragment ID
    if (Array.isArray(cfg.fragments)) {
      cfg.fragments.forEach((fragmentId: any, index: number) => {
        errors.push(
          ...validateStringField(fragmentId, `fragments[${index}]`, {
            minLength: 1,
          }),
        );
      });
    }
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(cfg);
}

/**
 * Validate variable definition
 * @param variable Variable definition
 * @param index Variable index
 * @returns Array of validation errors
 */
function validateVariableDefinition(
  variable: unknown,
  index: number,
): ValidationError[] {
  const errors: ValidationError[] = [];
  const prefix = `variables[${index}]`;

  // Validate required fields
  const requiredFields = ["name", "type", "required"];
  const variableRecord = variable as unknown as Record<string, unknown>;
  errors.push(...validateRequiredFields(variableRecord, requiredFields, prefix));

  // Validate name field
  if (variableRecord["name"] !== undefined) {
    errors.push(
      ...validateStringField(variableRecord["name"] as string, `${prefix}.name`, {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Validate type field
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

  // Validate required field
  if (
    variableRecord["required"] !== undefined &&
    typeof variableRecord["required"] !== "boolean"
  ) {
    errors.push(
      new SchemaValidationError(`${prefix}.required must be a boolean`, {
        field: `${prefix}.required`,
        value: variableRecord["required"],
      }),
    );
  }

  // Validate description field (if exists)
  if (variableRecord["description"] !== undefined) {
    errors.push(
      ...validateStringField(
        variableRecord["description"] as string,
        `${prefix}.description`,
        {
          maxLength: 500,
        },
      ),
    );
  }

  return errors;
}
