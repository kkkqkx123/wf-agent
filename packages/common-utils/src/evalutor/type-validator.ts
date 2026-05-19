/**
 * Type Validator - Type Validation Utilities
 * Provides runtime type checking for expression evaluation
 */

import { getGlobalLogger } from "../logger/index.js";

const logger = getGlobalLogger().child("TypeValidator", { pkg: "common-utils" });

/**
 * Validate comparison types before performing comparisons
 * @param operator Comparison operator
 * @param leftValue Left operand value
 * @param rightValue Right operand value
 * @param context Additional context information (method, propertyPath)
 * @returns true if types are valid for comparison, false otherwise
 */
export function validateComparisonTypes(
  operator: string,
  leftValue: unknown,
  rightValue: unknown,
  context: { method?: string; propertyPath?: string } = {},
): boolean {
  // For numeric comparisons, both sides must be numbers
  if ([">", "<", ">=", "<="].includes(operator)) {
    if (typeof leftValue !== "number" || typeof rightValue !== "number") {
      logger.warn(
        `Numeric comparison requires both operands to be numbers`,
        {
          operator,
          leftType: typeof leftValue,
          rightType: typeof rightValue,
          leftValue,
          rightValue,
          ...context,
        },
      );
      return false;
    }
  }

  // For equality, warn about type mismatches
  if (["==", "!="].includes(operator)) {
    if (
      typeof leftValue !== typeof rightValue &&
      leftValue !== null &&
      rightValue !== null &&
      leftValue !== undefined &&
      rightValue !== undefined
    ) {
      logger.warn(
        `Comparing different types may produce unexpected results`,
        {
          operator,
          leftType: typeof leftValue,
          rightType: typeof rightValue,
          leftValue,
          rightValue,
          ...context,
        },
      );
      // Still allow the comparison to proceed, just warn
    }
  }

  return true;
}

/**
 * Validate array method result types
 * @param method Array method name
 * @param result Method result
 * @param expectedType Expected result type
 * @param context Additional context
 * @returns true if type is valid
 */
export function validateArrayMethodResult(
  method: string,
  result: unknown,
  expectedType: "boolean" | "number" | "object" | "any",
  context: { arrayPath?: string; propertyName?: string } = {},
): boolean {
  if (expectedType === "any") {
    return true;
  }

  const actualType = result === null ? "null" : typeof result;

  if (expectedType === "boolean" && typeof result !== "boolean") {
    logger.warn(
      `Array method ${method} expected boolean result, got ${actualType}`,
      {
        method,
        expectedType,
        actualType,
        result,
        ...context,
      },
    );
    return false;
  }

  if (expectedType === "number" && typeof result !== "number") {
    logger.warn(
      `Array method ${method} expected number result, got ${actualType}`,
      {
        method,
        expectedType,
        actualType,
        result,
        ...context,
      },
    );
    return false;
  }

  if (expectedType === "object" && (typeof result !== "object" || result === null)) {
    logger.warn(
      `Array method ${method} expected object result, got ${actualType}`,
      {
        method,
        expectedType,
        actualType,
        result,
        ...context,
      },
    );
    return false;
  }

  return true;
}

/**
 * Check if a value is a valid number for arithmetic operations
 * @param value Value to check
 * @param context Context information
 * @returns true if value is a valid number
 */
export function isValidNumber(value: unknown, context: { operation?: string } = {}): boolean {
  if (typeof value !== "number") {
    logger.warn(
      `Expected number for operation, got ${typeof value}`,
      {
        value,
        valueType: typeof value,
        ...context,
      },
    );
    return false;
  }

  if (isNaN(value) || !isFinite(value)) {
    logger.warn(
      `Invalid number value for operation`,
      {
        value,
        isNaN: isNaN(value),
        isFinite: isFinite(value),
        ...context,
      },
    );
    return false;
  }

  return true;
}

/**
 * Validate that a value is an array
 * @param value Value to check
 * @param context Context information
 * @returns true if value is an array
 */
export function isArray(value: unknown, context: { path?: string } = {}): value is unknown[] {
  if (!Array.isArray(value)) {
    logger.warn(
      `Expected array, got ${typeof value}`,
      {
        value,
        valueType: typeof value,
        isArray: Array.isArray(value),
        ...context,
      },
    );
    return false;
  }
  return true;
}
