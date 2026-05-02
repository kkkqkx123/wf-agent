/**
 * Validation helper functions
 * Provides low-level field validation utilities for configuration validators
 */

import { ValidationError, SchemaValidationError } from "@wf-agent/types";

/**
 * Verify required fields
 * @param obj Object
 * @param requiredFields List of required fields
 * @param fieldName Field name (used for error messages)
 * @returns Array of validation errors
 */
export function validateRequiredFields(
  obj: Record<string, unknown>,
  requiredFields: string[],
  fieldName: string,
): ValidationError[] {
  const errors: ValidationError[] = [];

  for (const field of requiredFields) {
    if (obj[field] === undefined || obj[field] === null || obj[field] === "") {
      errors.push(
        new SchemaValidationError(`${fieldName}.${field} is required`, {
          field: `${fieldName}.${field}`,
          value: obj[field],
        }),
      );
    }
  }

  return errors;
}

/**
 * Verify string field
 * @param value: Field value
 * @param fieldName: Field name
 * @param options: Verification options
 * @returns: Array of verification errors
 */
export function validateStringField(
  value: unknown,
  fieldName: string,
  options?: {
    minLength?: number;
    maxLength?: number;
    pattern?: RegExp;
  },
): ValidationError[] {
  const errors: ValidationError[] = [];

  if (typeof value !== "string") {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a string`, {
        field: fieldName,
        value: value,
      }),
    );
    return errors;
  }

  if (options?.minLength && value.length < options.minLength) {
    errors.push(
      new SchemaValidationError(`${fieldName} length must be at least ${options.minLength}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (options?.maxLength && value.length > options.maxLength) {
    errors.push(
      new SchemaValidationError(`${fieldName} length must not exceed ${options.maxLength}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (options?.pattern && !options.pattern.test(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} format is invalid`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  return errors;
}

/**
 * Verify a numeric field
 * @param value: The field value
 * @param fieldName: The field name
 * @param options: Verification options
 * @returns: An array of verification errors
 */
export function validateNumberField(
  value: unknown,
  fieldName: string,
  options?: {
    min?: number;
    max?: number;
    integer?: boolean;
  },
): ValidationError[] {
  const errors: ValidationError[] = [];

  if (typeof value !== "number" || isNaN(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a number`, {
        field: fieldName,
        value: value,
      }),
    );
    return errors;
  }

  if (options?.integer && !Number.isInteger(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be an integer`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (options?.min !== undefined && value < options.min) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be at least ${options.min}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (options?.max !== undefined && value > options.max) {
    errors.push(
      new SchemaValidationError(`${fieldName} must not exceed ${options.max}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  return errors;
}

/**
 * Verify boolean field
 * @param value Field value
 * @param fieldName Field name
 * @returns Array of verification errors
 */
export function validateBooleanField(value: unknown, fieldName: string): ValidationError[] {
  const errors: ValidationError[] = [];

  if (typeof value !== "boolean") {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a boolean`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  return errors;
}

/**
 * Verify array field
 * @param value: Field value
 * @param fieldName: Field name
 * @param options: Verification options
 * @returns: Array of verification errors
 */
export function validateArrayField(
  value: unknown,
  fieldName: string,
  options?: {
    minLength?: number;
    maxLength?: number;
  },
): ValidationError[] {
  const errors: ValidationError[] = [];

  if (!Array.isArray(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be an array`, {
        field: fieldName,
        value: value,
      }),
    );
    return errors;
  }

  if (options?.minLength && value.length < options.minLength) {
    errors.push(
      new SchemaValidationError(`${fieldName} length must be at least ${options.minLength}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (options?.maxLength && value.length > options.maxLength) {
    errors.push(
      new SchemaValidationError(`${fieldName} length must not exceed ${options.maxLength}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  return errors;
}

/**
 * Verify object field
 * @param value: Field value
 * @param fieldName: Field name
 * @returns: Array of verification errors
 */
export function validateObjectField(value: unknown, fieldName: string): ValidationError[] {
  const errors: ValidationError[] = [];

  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be an object`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  return errors;
}

/**
 * Verify enum field
 * @param value Field value
 * @param fieldName Field name
 * @param enumValues Array of enum values
 * @returns Array of validation errors
 */
export function validateEnumField(
  value: unknown,
  fieldName: string,
  enumValues: unknown[],
): ValidationError[] {
  const errors: ValidationError[] = [];

  if (!enumValues.includes(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be one of: ${enumValues.join(", ")}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  return errors;
}
