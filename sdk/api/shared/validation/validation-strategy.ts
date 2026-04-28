/**
 * Functional validator
 * Provides unified validation functions, returning Result<T, ValidationError[]> type
 */

import { ValidationError, SchemaValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Verify required fields
 * @param data Data object
 * @param fields List of required fields
 * @param fieldName Field name (used for error messages)
 * @returns Array of verification errors
 */
export function validateRequiredFields<T>(
  data: T,
  fields: (keyof T)[],
  fieldName: string,
): Result<T, ValidationError[]> {
  const errors: ValidationError[] = [];

  for (const field of fields) {
    const value = data[field];
    if (value === null || value === undefined || value === "") {
      errors.push(
        new SchemaValidationError(`${String(field)} cannot be empty`, {
          field: `${fieldName}.${String(field)}`,
          value: value,
        }),
      );
    }
  }

  if (errors.length === 0) {
    return ok(data);
  } else {
    return err(errors);
  }
}

/**
 * Verify string length
 * @param value String value
 * @param fieldName Field name
 * @param min Minimum length
 * @param max Maximum length
 * @returns Array of validation errors
 */
export function validateStringLength(
  value: string,
  fieldName: string,
  min: number,
  max: number,
): Result<string, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (typeof value !== "string") {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a string`, {
        field: fieldName,
        value: value,
      }),
    );
    return err(errors);
  }

  if (value.length < min) {
    errors.push(
      new SchemaValidationError(`${fieldName} length must be at least ${min}`, {
        field: fieldName,
        value: value,
      }),
    );
  }
  if (value.length > max) {
    errors.push(
      new SchemaValidationError(`${fieldName} length must not exceed ${max}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (errors.length === 0) {
    return ok(value);
  } else {
    return err(errors);
  }
}

/**
 * Verify positive number
 * @param value A numeric value
 * @param fieldName The name of the field
 * @returns An array of verification errors
 */
export function validatePositiveNumber(
  value: number,
  fieldName: string,
): Result<number, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (typeof value !== "number" || isNaN(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a number`, {
        field: fieldName,
        value: value,
      }),
    );
    return err(errors);
  }

  if (value < 0) {
    errors.push(
      new SchemaValidationError(`${fieldName} cannot be a negative number`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (errors.length === 0) {
    return ok(value);
  } else {
    return err(errors);
  }
}

/**
 * Verify object structure
 * @param value Object value
 * @param fieldName Field name
 * @returns Array of verification errors
 */
export function validateObject(
  value: unknown,
  fieldName: string,
): Result<Record<string, unknown>, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (value === null || value === undefined) {
    errors.push(
      new SchemaValidationError(`${fieldName} cannot be empty`, {
        field: fieldName,
        value: value,
      }),
    );
  } else if (typeof value !== "object" || Array.isArray(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a valid object`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (errors.length === 0) {
    return ok(value as Record<string, unknown>);
  } else {
    return err(errors);
  }
}

/**
 * Verify Array
 * @param value Array value
 * @param fieldName Field name
 * @param minLength Minimum length
 * @returns Array with validation errors
 */
export function validateArray(
  value: unknown[],
  fieldName: string,
  minLength: number = 1,
): Result<unknown[], ValidationError[]> {
  const errors: ValidationError[] = [];

  if (!Array.isArray(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be an array`, {
        field: fieldName,
        value: value,
      }),
    );
  } else if (value.length < minLength) {
    errors.push(
      new SchemaValidationError(`${fieldName} requires at least ${minLength} elements`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (errors.length === 0) {
    return ok(value);
  } else {
    return err(errors);
  }
}

/**
 * Verify Boolean value
 * @param value Boolean value
 * @param fieldName Field name
 * @returns Array of verification errors
 */
export function validateBoolean(
  value: unknown,
  fieldName: string,
): Result<boolean, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (typeof value !== "boolean") {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a boolean`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (errors.length === 0) {
    return ok(value as boolean);
  } else {
    return err(errors);
  }
}

/**
 * Verify regular expression match
 * @param value String value
 * @param fieldName Field name
 * @param regex Regular expression
 * @param message Custom error message
 * @returns Array of validation errors
 */
export function validatePattern(
  value: string,
  fieldName: string,
  regex: RegExp,
  message?: string,
): Result<string, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (typeof value !== "string") {
    errors.push(
      new SchemaValidationError(`${fieldName} must be a string`, {
        field: fieldName,
        value: value,
      }),
    );
    return err(errors);
  }

  if (!regex.test(value)) {
    errors.push(
      new SchemaValidationError(message || `${fieldName} format is invalid`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (errors.length === 0) {
    return ok(value);
  } else {
    return err(errors);
  }
}

/**
 * Verify enum values
 * @param value The value to be checked
 * @param fieldName The name of the field
 * @param enumValues An array of enum values
 * @returns An array of verification errors
 */
export function validateEnum<T>(
  value: T,
  fieldName: string,
  enumValues: T[],
): Result<T, ValidationError[]> {
  const errors: ValidationError[] = [];

  if (!enumValues.includes(value)) {
    errors.push(
      new SchemaValidationError(`${fieldName} must be one of: ${enumValues.join(", ")}`, {
        field: fieldName,
        value: value,
      }),
    );
  }

  if (errors.length === 0) {
    return ok(value);
  } else {
    return err(errors);
  }
}
