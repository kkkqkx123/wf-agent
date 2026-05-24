/**
 * SecurityValidator - Security Validation Tool
 * Provides security validation for paths and expressions to prevent injection attacks.
 *
 * Security Measures:
 * - Path depth limitation
 * - Access to special properties is prohibited (to prevent prototype chain contamination)
 * - Path format validation
 * - Expression length limitation
 *
 * Note: Special template variables (@index, @first, @last, this) are processed by the template renderer and do not pass through this security validator.
 *
 */

import { RuntimeValidationError } from "@wf-agent/types";

/**
 * Security Configuration
 */
export const SECURITY_CONFIG = {
  /** Maximum length of the expression */
  MAX_EXPRESSION_LENGTH: 1000,
  /** Maximum path depth */
  MAX_PATH_DEPTH: 10,
  /** Forbidden properties (to prevent prototype chain contamination) */
  FORBIDDEN_PROPERTIES: ["__proto__", "constructor", "prototype"],
  /**
   * Valid path character pattern
   * Supports regular variable paths: user.name, items[0], data.items[0].name
   */
  VALID_PATH_PATTERN: /^[a-zA-Z_][a-zA-Z0-9_]*(\[\d+\])?(\.[a-zA-Z_][a-zA-Z0-9_]*(\[\d+\])?)*$/,
} as const;

/**
 * Verify the safety of the expression
 * @param expression: The expression as a string
 * @throws ValidationError: If the expression is unsafe
 */
export function validateExpression(expression: string): void {
  if (!expression || typeof expression !== "string") {
    throw new RuntimeValidationError("Expression must be a non-empty string", {
      operation: "validateExpression",
      field: "expression",
      value: expression,
    });
  }

  if (expression.length > SECURITY_CONFIG.MAX_EXPRESSION_LENGTH) {
    throw new RuntimeValidationError(
      `Expression length exceeds maximum limit of ${SECURITY_CONFIG.MAX_EXPRESSION_LENGTH}`,
      {
        operation: "validateExpression",
        field: "expression",
        value: expression.substring(0, 100),
      },
    );
  }
}

/**
 * Verify path security
 * @param path Path string
 * @throws ValidationError If the path is not secure
 */
export function validatePath(path: string): void {
  // Check if it is a string.
  if (!path || typeof path !== "string") {
    throw new RuntimeValidationError("Path must be a non-empty string", {
      operation: "validatePath",
      field: "path",
      value: path,
    });
  }

  // Check if it contains any prohibited attributes.
  for (const forbidden of SECURITY_CONFIG.FORBIDDEN_PROPERTIES) {
    if (path.includes(forbidden)) {
      throw new RuntimeValidationError(`Path contains forbidden property: ${forbidden}`, {
        operation: "validatePath",
        field: "path",
        value: path,
      });
    }
  }

  // Verify path format using regex pattern
  if (!SECURITY_CONFIG.VALID_PATH_PATTERN.test(path)) {
    throw new RuntimeValidationError(
      "Path contains invalid characters. Only alphanumeric, underscore, dot, and array brackets are allowed",
      {
        operation: "validatePath",
        field: "path",
        value: path,
      },
    );
  }

  // Check the path depth.
  const depth = path.split(".").length;
  if (depth > SECURITY_CONFIG.MAX_PATH_DEPTH) {
    throw new RuntimeValidationError(
      `Path depth exceeds maximum limit of ${SECURITY_CONFIG.MAX_PATH_DEPTH}`,
      {
        operation: "validatePath",
        field: "path",
        value: path,
      },
    );
  }
}

/**
 * Verify whether the array index is within the valid range.
 * @param array: The array
 * @param index: The index
 * @throws ValidationError: If the index is out of bounds
 */
export function validateArrayIndex(array: unknown[], index: number): void {
  if (!Array.isArray(array)) {
    throw new RuntimeValidationError("Target is not an array", {
      operation: "validateArrayAccess",
      field: "array",
      value: array,
    });
  }

  if (!Number.isInteger(index) || index < 0 || index >= array.length) {
    throw new RuntimeValidationError(
      `Array index ${index} out of bounds. Array length is ${array.length}`,
      {
        operation: "validateArrayAccess",
        field: "index",
        value: index,
      },
    );
  }
}

/**
 * Verify if the value is of an allowed basic type.
 * @param value The value to be checked
 * @throws ValidationError If the value type is not allowed
 */
export function validateValueType(value: unknown): void {
  if (value === null || value === undefined) {
    return; // null and undefined are allowed
  }

  const typeName = typeof value;

  if (typeName === "function") {
    throw new RuntimeValidationError(`Value type ${typeName} is not allowed`, {
      operation: "validateType",
      field: "value",
      value: value,
    });
  }

  if (typeName === "string" || typeName === "number" || typeName === "boolean") {
    return; // primitive types are allowed
  }

  if (Array.isArray(value)) {
    return; // arrays are allowed
  }

  if (typeName === "object") {
    const obj = value as Record<string, unknown>;
    const constructor = obj?.constructor;
    if (constructor && constructor !== Object) {
      throw new RuntimeValidationError(`Value type ${constructor.name} is not allowed`, {
        operation: "validateType",
        field: "value",
        value: value,
      });
    }
    return; // plain objects are allowed
  }
}
