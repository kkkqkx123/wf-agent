/**
 * Shared Validation Utilities for Command Classes
 *
 * This module provides reusable validation functions to eliminate code duplication
 * across all Command implementations in the SDK API layer.
 *
 * Benefits:
 * - Reduces code duplication (~46 validation messages)
 * - Standardizes error messages across all commands
 * - Makes validation logic easier to maintain and update
 * - Provides consistent validation patterns
 */

/**
 * Validate a required string field (non-empty after trim)
 *
 * @param value - The string value to validate
 * @param fieldName - The name of the field for error messages
 * @returns Array of error messages (empty if valid)
 *
 * @example
 * ```typescript
 * const errors = validateRequiredString(name, "User name");
 * // Returns: ["User name cannot be empty."] if name is empty
 * ```
 */
export function validateRequiredString(value: string | undefined, fieldName: string): string[] {
  if (!value || value.trim().length === 0) {
    return [`${fieldName} cannot be empty.`];
  }
  return [];
}

/**
 * Validate a required ID field (string or number)
 *
 * Checks both for existence and (for strings) for non-empty after trim.
 *
 * @param id - The ID to validate (string or number)
 * @param fieldName - The name of the field for error messages (default: "ID")
 * @returns Array of error messages (empty if valid)
 *
 * @example
 * ```typescript
 * const errors = validateRequiredId(userId, "User ID");
 * // Returns: ["User ID must be provided."] if userId is undefined/null
 * // Returns: ["User ID cannot be empty."] if userId is empty string
 * ```
 */
export function validateRequiredId<T extends string | number>(
  id: T | undefined,
  fieldName: string = "ID",
): string[] {
  if (id === undefined || id === null) {
    return [`${fieldName} must be provided.`];
  }
  if (typeof id === "string" && id.trim().length === 0) {
    return [`${fieldName} cannot be empty.`];
  }
  return [];
}

/**
 * Validate an optional positive integer field
 *
 * If the value is provided, it must be a positive integer (>= 1).
 *
 * @param value - The number to validate
 * @param fieldName - The name of the field for error messages
 * @returns Array of error messages (empty if valid or undefined)
 *
 * @example
 * ```typescript
 * const errors = validateOptionalPositiveInt(maxRetries, "Max retries");
 * // Returns: [] if maxRetries is undefined
 * // Returns: ["Max retries must be a positive integer."] if maxRetries <= 0 or not integer
 * ```
 */
export function validateOptionalPositiveInt(value: number | undefined, fieldName: string): string[] {
  if (value !== undefined && value !== null) {
    if (!Number.isInteger(value) || value < 1) {
      return [`${fieldName} must be a positive integer.`];
    }
  }
  return [];
}

/**
 * Combine multiple error arrays into a single flat array
 *
 * Useful for aggregating errors from multiple validation checks.
 *
 * @param errorArrays - Multiple arrays of error strings
 * @returns Flattened array of all errors
 *
 * @example
 * ```typescript
 * const allErrors = combineErrors(
 *   validateRequiredString(name, "Name"),
 *   validateOptionalPositiveInt(age, "Age"),
 *   validateRequiredId(userId, "User ID")
 * );
 * ```
 */
export function combineErrors(...errorArrays: string[][]): string[] {
  return errorArrays.flat();
}

/**
 * Validate a required object/entity is provided
 *
 * @param entity - The entity to validate
 * @param fieldName - The name of the field for error messages
 * @returns Array of error messages (empty if valid)
 *
 * @example
 * ```typescript
 * const errors = validateRequiredEntity(config, "Configuration");
 * // Returns: ["Configuration must be provided."] if config is null/undefined
 * ```
 */
export function validateRequiredEntity<T>(entity: T | undefined, fieldName: string): string[] {
  if (!entity) {
    return [`${fieldName} must be provided.`];
  }
  return [];
}
