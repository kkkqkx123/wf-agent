/**
 * CommandValidator - A command parameter validation utility class
 *
 * Provides a chainable API for verifying command parameters
 * Simplifies the validation logic and reduces duplicate code
 */

import type { CommandValidationResult } from "../types/command.js";
import { validationSuccess, validationFailure } from "../types/command.js";

/**
 * Command Validator
 * Provides a chained validation approach
 */
export class CommandValidator {
  private errors: string[] = [];

  /**
   * Verify that the value is not empty.
   * @param value The value to be verified
   * @param fieldName The name of the field
   * @returns this
   */
  notEmpty(value: unknown, fieldName: string): this {
    if (
      value === null ||
      value === undefined ||
      (typeof value === "string" && value.trim().length === 0)
    ) {
      this.errors.push(`${fieldName} cannot be empty`);
    }
    return this;
  }

  /**
   * Verify that the array is not empty
   * @param array The array to be verified
   * @param fieldName The field name
   * @returns this
   */
  notEmptyArray(array: unknown[], fieldName: string): this {
    if (!array || array.length === 0) {
      this.errors.push(`${fieldName} cannot be empty`);
    }
    return this;
  }

  /**
   * Verify if a number is within a specified range
   * @param value The number to be verified
   * @param fieldName The name of the field
   * @param min The minimum value
   * @param max The maximum value
   * @returns This
   */
  inRange(value: number, fieldName: string, min: number, max: number): this {
    if (value < min || value > max) {
      this.errors.push(`${fieldName} must be between ${min} and ${max}`);
    }
    return this;
  }

  /**
   * Verify that a number is greater than or equal to a specified value.
   * @param value The number to be verified
   * @param fieldName The name of the field
   * @param min The minimum value
   * @returns this
   */
  min(value: number, fieldName: string, min: number): this {
    if (value < min) {
      this.errors.push(`${fieldName} must be greater than or equal to ${min}`);
    }
    return this;
  }

  /**
   * Verify that a number is less than or equal to a specified value.
   * @param value The number to be verified
   * @param fieldName The name of the field
   * @param max The maximum value
   * @returns this
   */
  max(value: number, fieldName: string, max: number): this {
    if (value > max) {
      this.errors.push(`${fieldName} must be less than or equal to ${max}`);
    }
    return this;
  }

  /**
   * Verify string length
   * @param value The string to be verified
   * @param fieldName The name of the field
   * @param minLength The minimum length
   * @param maxLength The maximum length
   * @returns This
   */
  length(value: string, fieldName: string, minLength: number, maxLength: number): this {
    if (value.length < minLength || value.length > maxLength) {
      this.errors.push(`${fieldName} length must be between ${minLength} and ${maxLength}`);
    }
    return this;
  }

  /**
   * Verify if a string matches a regular expression
   * @param value The string to be verified
   * @param fieldName The name of the field
   * @param pattern The regular expression
   * @param errorMessage A custom error message
   * @returns this
   */
  matches(value: string, fieldName: string, pattern: RegExp, errorMessage?: string): this {
    if (!pattern.test(value)) {
      this.errors.push(errorMessage || `${fieldName} format is invalid`);
    }
    return this;
  }

  /**
   * Verify if the value is in the specified list.
   * @param value The value to be verified
   * @param fieldName The name of the field
   * @param allowedValues The list of allowed values
   * @returns this
   */
  oneOf(value: unknown, fieldName: string, allowedValues: unknown[]): this {
    if (!allowedValues.includes(value)) {
      this.errors.push(`${fieldName} must be one of: ${allowedValues.join(", ")}`);
    }
    return this;
  }

  /**
   * Custom Validation
   * @param condition The validation condition
   * @param errorMessage The error message
   * @returns this
   */
  custom(condition: boolean, errorMessage: string): this {
    if (!condition) {
      this.errors.push(errorMessage);
    }
    return this;
  }

  /**
   * Get the validation result
   * @returns The validation result
   */
  getResult(): CommandValidationResult {
    return this.errors.length > 0 ? validationFailure(this.errors) : validationSuccess();
  }

  /**
   * Check for any errors
   * @returns Whether there are any errors
   */
  hasErrors(): boolean {
    return this.errors.length > 0;
  }

  /**
   * Get all error messages
   * @returns Array of error messages
   */
  getErrors(): string[] {
    return [...this.errors];
  }
}
