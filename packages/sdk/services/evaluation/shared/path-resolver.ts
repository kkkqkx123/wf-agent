/**
 * PathResolver - Path Resolver
 * Provides simple object path resolving functionality
 *
 * Supported path formats:
 * - Nested object access: e.g. "user.name", "output.data.items"
 * - Array index access: "items[0]", "items[0].name".
 * - Combined access: e.g. "output.data.items[0].name"
 * - Context-aware access: "input.x", "output.x", "variables.x"
 *
 * Usage example:
 * - resolvePath("user.name", obj) - get nested attribute values
 * - resolvePath("items[0].name", obj) - get the array element property
 * - setPath("user.name", obj, "John") - sets the value of the nested attribute
 * - pathExists("user.name", obj) - check if the path exists
 * - resolveContextPath("input.x", context) - resolve from evaluation context
 */

import type { EvaluationContext } from "@wf-agent/types";
import { validatePath } from "./security-validator.js";

/**
 * Parse the path and get the value
 * @param path Path string, supports nested access and array indexing, e.g. "user.name", "items[0].name"
 * @param root Root object
 * @returns the value of the path, or undefined if the path does not exist.
 */
export function resolvePath(path: string, root: unknown): unknown {
  if (!path || !root) {
    return undefined;
  }

  // Verify path security
  validatePath(path);

  // Support for nested path access, e.g. "output.data.items[0].name"
  const parts = path.split(".");
  let value: unknown = root;

  for (const part of parts) {
    if (value === null || value === undefined) {
      return undefined;
    }

    // Handles array index access, e.g. items[0].
    const arrayMatch = part.match(/(\w+)\[(\d+)\]/);
    if (arrayMatch && arrayMatch[1] && arrayMatch[2]) {
      const arrayName = arrayMatch[1];
      const index = parseInt(arrayMatch[2], 10);
      value = (value as Record<string, unknown>)[arrayName];
      if (Array.isArray(value)) {
        value = value[index];
      }
    } else {
      value = (value as Record<string, unknown>)[part];
    }
  }

  return value;
}

/**
 * Checking if a path exists
 * @param path path string
 * @param root The root object
 * @returns if the path exists
 */
export function pathExists(path: string, root: unknown): boolean {
  try {
    return resolvePath(path, root) !== undefined;
  } catch {
    return false;
  }
}

/**
 * Set the value of path
 * @param path path string
 * @param root The root object
 * @param value The value to set.
 * @returns Whether the setting was successful or not
 */
export function setPath(path: string, root: unknown, value: unknown): boolean {
  if (!path || !root) {
    return false;
  }

  // First check if the path contains empty parts (e.g. "valid..invalid")
  const parts = path.split(".");
  for (const part of parts) {
    if (!part) {
      return false; // Contains empty parts
    }
  }

  // Verify path security (property names beginning with a number throw an exception)
  validatePath(path);
  if (parts.length === 0) {
    return false;
  }

  let current: unknown = root;

  // Iterate to the penultimate level
  for (let i = 0; i < parts.length - 1; i++) {
    const part = parts[i];
    if (!part) {
      return false; // Include empty parts, e.g. "valid...invalid"
    }

    // Handling Array Index Access
    const arrayMatch = part.match(/(\w+)\[(\d+)\]/);
    if (arrayMatch && arrayMatch[1] && arrayMatch[2]) {
      const arrayName = arrayMatch[1];
      const index = parseInt(arrayMatch[2], 10);

      if (!(current as Record<string, unknown>)[arrayName]) {
        (current as Record<string, unknown>)[arrayName] = [];
      }
      current = (current as Record<string, unknown>)[arrayName];

      if (!Array.isArray(current)) {
        return false;
      }

      // If the index is out of range, extend the array
      while ((current as unknown[]).length <= index) {
        (current as unknown[]).push({});
      }
      current = (current as unknown[])[index];
    } else {
      if (!(current as Record<string, unknown>)[part]) {
        (current as Record<string, unknown>)[part] = {};
      }
      current = (current as Record<string, unknown>)[part];
      if (typeof current !== "object" || current === null) {
        return false;
      }
    }
  }

  // Setting the value of the last layer
  const lastPart = parts[parts.length - 1];
  if (!lastPart || typeof current !== "object" || current === null) {
    return false;
  }

  const arrayMatch = lastPart.match(/(\w+)\[(\d+)\]/);
  if (arrayMatch && arrayMatch[1] && arrayMatch[2]) {
    const arrayName = arrayMatch[1];
    const index = parseInt(arrayMatch[2], 10);

    if (!(current as Record<string, unknown>)[arrayName]) {
      (current as Record<string, unknown>)[arrayName] = [];
    }

    // If the index is out of range, extend the array
    while (((current as Record<string, unknown>)[arrayName] as unknown[]).length <= index) {
      ((current as Record<string, unknown>)[arrayName] as unknown[]).push(undefined);
    }
    ((current as Record<string, unknown>)[arrayName] as unknown[])[index] = value;
  } else {
    (current as Record<string, unknown>)[lastPart] = value;
  }

  return true;
}

/**
 * Find and update an item in an array by matching a key field.
 * Designed for data structures like `workflowExecution.variables` (array of objects with .name, .value, etc.).
 *
 * @param array - Array of objects to search
 * @param keyField - The field name to match against (e.g. "name")
 * @param keyValue - The value to match (e.g. "myVariable")
 * @param valueField - The field name to update (e.g. "value")
 * @param newValue - The new value to set
 * @returns true if item was found and updated, false otherwise
 */
export function setArrayItemByKey(
  array: Record<string, unknown>[],
  keyField: string,
  keyValue: string,
  valueField: string,
  newValue: unknown,
): boolean {
  if (!Array.isArray(array) || !keyField || !valueField) {
    return false;
  }

  const item = array.find(item => item[keyField] === keyValue);
  if (!item) {
    return false;
  }

  item[valueField] = newValue;
  return true;
}

/**
 * Unified path resolver for evaluation context
 * Resolves paths that can reference input, output, or variables scopes
 *
 * Examples:
 * - "input" → entire input object
 * - "input.data.items" → nested path in input
 * - "output.result" → nested path in output
 * - "variables.x" → variable named x
 * - "x" → defaults to variables scope
 * - "items[0].name" → array access
 *
 * @param path Path string with optional scope prefix
 * @param context Evaluation context
 * @returns The resolved value, or undefined if not found
 * @throws ExpressionSecurityError if path fails security validation
 */
export function resolveContextPath(path: string, context: EvaluationContext): unknown {
  if (!path || !context) {
    return undefined;
  }

  // Validate entire path upfront for security
  validatePath(path);

  // Handle root-level scope requests (safe due to hard-coded values)
  if (path === "input") return context.input;
  if (path === "output") return context.output;
  if (path === "variables") return context.variables;

  // Determine scope from prefix
  let scope: "input" | "output" | "variables" = "variables";
  let remaining = path;

  if (path.startsWith("input.")) {
    scope = "input";
    remaining = path.substring(6);
  } else if (path.startsWith("output.")) {
    scope = "output";
    remaining = path.substring(7);
  } else if (path.startsWith("variables.")) {
    scope = "variables";
    remaining = path.substring(10);
  }

  const root = context[scope];

  // If root scope is null/undefined, return undefined
  if (root === null || root === undefined) {
    return undefined;
  }

  if (!remaining) return root;

  // Resolve nested path without re-validating (already validated above)
  // Use resolvePath but skip the validatePath call by directly implementing the logic
  return resolvePathWithoutValidation(remaining, root);
}

/**
 * Internal helper to resolve path without re-validation
 * Used by resolveContextPath after initial validation
 * @internal
 */
function resolvePathWithoutValidation(path: string, root: unknown): unknown {
  const parts = path.split(".");
  let value: unknown = root;

  for (const part of parts) {
    if (value === null || value === undefined) {
      return undefined;
    }

    const arrayMatch = part.match(/(\w+)\[(\d+)\]/);
    if (arrayMatch && arrayMatch[1] && arrayMatch[2]) {
      const arrayName = arrayMatch[1];
      const index = parseInt(arrayMatch[2], 10);
      value = (value as Record<string, unknown>)[arrayName];
      if (Array.isArray(value)) {
        value = value[index];
      }
    } else {
      value = (value as Record<string, unknown>)[part];
    }
  }

  return value;
}
