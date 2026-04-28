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

import { z } from "zod";
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
   * 允许的路径字符正则
   * 支持常规变量路径：user.name, items[0], data.items[0].name
   */
  VALID_PATH_PATTERN: /^[a-zA-Z_][a-zA-Z0-9_]*(\[\d+\])?(\.[a-zA-Z_][a-zA-Z0-9_]*(\[\d+\])?)*$/,
} as const;

/**
 * Expression Schema
 */
const expressionSchema = z
  .string()
  .min(1, "Expression must be a non-empty string")
  .max(
    SECURITY_CONFIG.MAX_EXPRESSION_LENGTH,
    `Expression length exceeds maximum limit of ${SECURITY_CONFIG.MAX_EXPRESSION_LENGTH}`,
  );

/**
 * Path Schema (Basic Format Validation)
 */
const pathSchema = z
  .string()
  .min(1, "Path must be a non-empty string")
  .regex(
    SECURITY_CONFIG.VALID_PATH_PATTERN,
    "Path contains invalid characters. Only alphanumeric, underscore, dot, and array brackets are allowed",
  );

/**
 * Value Type Schema
 */
const valueTypeSchema = z.union([
  z.string(),
  z.number(),
  z.boolean(),
  z.undefined(),
  z.null(),
  z.array(z.unknown()),
  z
    .record(z.string(), z.unknown())
    .refine(val => val.constructor === Object || val.constructor === undefined, {
      message: "Special objects (Date, RegExp, Map, Set, etc.) are not allowed",
    }),
]);

/**
 * Verify the safety of the expression
 * @param expression: The expression as a string
 * @throws ValidationError: If the expression is unsafe
 */
export function validateExpression(expression: string): void {
  const result = expressionSchema.safeParse(expression);
  if (!result.success) {
    const message = result.error.issues[0]?.message || "Expression validation failed";
    throw new RuntimeValidationError(message, {
      operation: "validateExpression",
      field: "expression",
      value: expression,
    });
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

  // Verify path format
  const result = pathSchema.safeParse(path);
  if (!result.success) {
    const message = result.error.issues[0]?.message || "Path validation failed";
    throw new RuntimeValidationError(message, {
      operation: "validatePath",
      field: "path",
      value: path,
    });
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
  const arraySchema = z.array(z.unknown());
  const arrayResult = arraySchema.safeParse(array);
  if (!arrayResult.success) {
    throw new RuntimeValidationError("Target is not an array", {
      operation: "validateArrayAccess",
      field: "array",
      value: array,
    });
  }

  const indexSchema = z
    .number()
    .int()
    .nonnegative()
    .max(array.length - 1);
  const indexResult = indexSchema.safeParse(index);
  if (!indexResult.success) {
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
  const result = valueTypeSchema.safeParse(value);
  if (!result.success) {
    let message: string;

    if (value !== null && value !== undefined) {
      const typeName = typeof value;
      if (typeName === "function") {
        message = `Value type ${typeName} is not allowed`;
      } else if (typeof value === "object" && value.constructor && value.constructor !== Object) {
        message = `Value type ${value.constructor.name} is not allowed`;
      } else {
        message = `Value type ${typeName} is not allowed`;
      }
    } else {
      message = result.error.issues[0]?.message ?? "Value type validation failed";
    }

    throw new RuntimeValidationError(message, {
      operation: "validateType",
      field: "value",
      value: value,
    });
  }
}
