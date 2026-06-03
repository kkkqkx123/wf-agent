/**
 * PathResolver - Path Resolver
 * Provides simple object path resolving functionality
 * 
 * Supported path formats:
 * - Nested object access: e.g. "user.name", "output.data.items"
 * - Array index access: "items[0]", "items[0].name".
 * - Combined access: e.g. "output.data.items[0].name"
 * 
 * Usage example:
 * - resolvePath("user.name", obj) - get nested attribute values
 * - resolvePath("items[0].name", obj) - get the array element property
 * - setPath("user.name", obj, "John") - sets the value of the nested attribute
 * - pathExists("user.name", obj) - check if the path exists
 */

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

  const item = array.find((item) => item[keyField] === keyValue);
  if (!item) {
    return false;
  }

  item[valueField] = newValue;
  return true;
}
