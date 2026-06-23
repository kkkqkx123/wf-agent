/**
 * Configuration Validator
 *
 * Provides centralized configuration validation utilities.
 * This module consolidates validation patterns used across processors.
 *
 * Design principles:
 * - Works with Zod schemas (reuses existing schemas)
 * - Returns structured validation results
 * - Pure functions (no side effects)
 * - Reusable across different config types
 */

import type { ZodSchema, z } from "zod";

/**
 * Validation result structure.
 */
export interface ValidationResult<T> {
  /**
   * Whether validation succeeded.
   */
  valid: boolean;

  /**
   * Validated and parsed data (only if valid).
   */
  data?: T;

  /**
   * List of validation error messages.
   */
  errors: string[];
}

/**
 * Validate configuration using a Zod schema.
 *
 * @param config - Configuration object to validate
 * @param schema - Zod schema to validate against
 * @returns Validation result with errors if any
 */
export function validateConfig<T>(
  config: unknown,
  schema: ZodSchema<T>,
): ValidationResult<T> {
  const result = schema.safeParse(config);

  if (result.success) {
    return {
      valid: true,
      data: result.data,
      errors: [],
    };
  }

  const errors = result.error.issues.map((issue: z.ZodIssue) => {
    const path = issue.path.length > 0 ? issue.path.join(".") : "root";
    return `${path}: ${issue.message}`;
  });

  return {
    valid: false,
    errors,
  };
}

/**
 * Validate configuration and throw if invalid.
 *
 * @param config - Configuration object to validate
 * @param schema - Zod schema to validate against
 * @param context - Context description for error message
 * @returns Validated data
 * @throws Error if validation fails
 */
export function validateConfigOrThrow<T>(
  config: unknown,
  schema: ZodSchema<T>,
  context: string = "Configuration",
): T {
  const result = validateConfig(config, schema);

  if (!result.valid) {
    throw new Error(
      `${context} validation failed:\n${result.errors.map((e) => `  - ${e}`).join("\n")}`,
    );
  }

  return result.data!;
}

/**
 * Validate multiple configurations and collect all errors.
 *
 * @param configs - Array of config validation inputs
 * @returns Combined validation result
 */
export function validateConfigs<T extends Record<string, unknown>>(
  configs: Array<{ name: string; config: unknown; schema: ZodSchema<T[keyof T]> }>,
): ValidationResult<Record<string, T[keyof T]>> {
  const errors: string[] = [];
  const data: Record<string, T[keyof T]> = {};

  for (const { name, config, schema } of configs) {
    const result = validateConfig(config, schema);

    if (!result.valid) {
      errors.push(...result.errors.map((e) => `[${name}] ${e}`));
    } else {
      data[name] = result.data!;
    }
  }

  return {
    valid: errors.length === 0,
    data: errors.length === 0 ? data : undefined,
    errors,
  };
}

/**
 * Configuration field validator.
 *
 * Provides field-level validation utilities.
 */
export class FieldValidator {
  /**
   * Validate that a required field is present and not empty.
   */
  static required(value: unknown, fieldName: string): string | null {
    if (value === undefined || value === null) {
      return `${fieldName} is required`;
    }
    if (typeof value === "string" && value.trim() === "") {
      return `${fieldName} cannot be empty`;
    }
    return null;
  }

  /**
   * Validate that a number is within a range.
   */
  static range(
    value: number,
    fieldName: string,
    min?: number,
    max?: number,
  ): string | null {
    if (min !== undefined && value < min) {
      return `${fieldName} must be at least ${min}`;
    }
    if (max !== undefined && value > max) {
      return `${fieldName} must be at most ${max}`;
    }
    return null;
  }

  /**
   * Validate that a value is one of allowed values.
   */
  static enum<T extends string>(
    value: string,
    fieldName: string,
    allowed: T[],
  ): string | null {
    if (!allowed.includes(value as T)) {
      return `${fieldName} must be one of: ${allowed.join(", ")}`;
    }
    return null;
  }

  /**
   * Validate that a string matches a pattern.
   */
  static pattern(
    value: string,
    fieldName: string,
    pattern: RegExp,
    patternDesc?: string,
  ): string | null {
    if (!pattern.test(value)) {
      return `${fieldName} must match pattern: ${patternDesc || pattern.source}`;
    }
    return null;
  }

  /**
   * Validate that arrays have no intersection.
   */
  static noIntersection(
    arr1: string[],
    arr2: string[],
    fieldName1: string,
    fieldName2: string,
  ): string | null {
    const intersection = arr1.filter((item) => arr2.includes(item));
    if (intersection.length > 0) {
      return `${fieldName1} and ${fieldName2} have intersection: ${intersection.join(", ")}`;
    }
    return null;
  }

  /**
   * Validate string length
   */
  static length(
    value: string,
    fieldName: string,
    min?: number,
    max?: number,
  ): string | null {
    if (min !== undefined && value.length < min) {
      return `${fieldName} must be at least ${min} characters`;
    }
    if (max !== undefined && value.length > max) {
      return `${fieldName} must be at most ${max} characters`;
    }
    return null;
  }

  /**
   * Validate URL format
   */
  static url(value: string, fieldName: string): string | null {
    try {
      new URL(value);
      return null;
    } catch {
      return `${fieldName} must be a valid URL`;
    }
  }

  /**
   * Validate email format
   */
  static email(value: string, fieldName: string): string | null {
    const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
    return emailRegex.test(value) ? null : `${fieldName} must be a valid email`;
  }

  /**
   * Validate date is not in the past
   */
  static notPast(value: Date, fieldName: string): string | null {
    if (value < new Date()) {
      return `${fieldName} cannot be in the past`;
    }
    return null;
  }

  /**
   * Validate JSON string
   */
  static json(value: string, fieldName: string): string | null {
    try {
      JSON.parse(value);
      return null;
    } catch {
      return `${fieldName} must be valid JSON`;
    }
  }

  /**
   * Validate that value is not empty or whitespace
   */
  static notEmpty(value: string, fieldName: string): string | null {
    if (!value || value.trim() === '') {
      return `${fieldName} cannot be empty`;
    }
    return null;
  }

  /**
   * Validate array is not empty
   */
  static arrayNotEmpty<T>(value: T[], fieldName: string): string | null {
    if (!Array.isArray(value) || value.length === 0) {
      return `${fieldName} cannot be empty`;
    }
    return null;
  }

  /**
   * Run multiple field validators and collect errors.
   */
  static validateFields(
    validators: Array<() => string | null>,
  ): { valid: boolean; errors: string[] } {
    const errors = validators
      .map((v) => v())
      .filter((e): e is string => e !== null);

    return {
      valid: errors.length === 0,
      errors,
    };
  }
}

/**
 * Create a composite validator from multiple validation functions.
 *
 * @param validators - Array of validation functions
 * @returns Composite validation function
 */
export function createCompositeValidator<T>(
  validators: Array<(config: T) => string | null>,
): (config: T) => ValidationResult<T> {
  return (config: T) => {
    const errors = validators
      .map((v) => v(config))
      .filter((e): e is string => e !== null);

    return {
      valid: errors.length === 0,
      data: errors.length === 0 ? config : undefined,
      errors,
    };
  };
}