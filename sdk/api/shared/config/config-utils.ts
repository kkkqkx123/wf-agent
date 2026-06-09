/**
 * Configuration Utility Functions
 *
 * Provides utility functions for configuration processing.
 * Focus on parameter substitution and transformation logic.
 *
 * Note: File I/O operations (loadAgentLoopConfig) have been moved to
 * apps/config-processor to keep the SDK I/O-free.
 */

/**
 * Substitute parameters in an object by replacing {{parameters.xxx}} placeholders.
 *
 * This function is idempotent for empty parameters - it returns the original
 * object unchanged if no parameters are provided, so callers don't need to
 * check for empty parameters before calling.
 *
 * @param obj The object to process
 * @param parameters The parameter object containing replacement values
 * @returns A new object with parameters substituted (or original if no params)
 */
export function substituteParameters<T>(
  obj: T,
  parameters?: Record<string, unknown>,
): T {
  // If no parameters, return original object (idempotent for empty params)
  if (!parameters || Object.keys(parameters).length === 0) {
    return obj;
  }

  // Deep clone using structuredClone (Node.js 17+)
  // Falls back to JSON serialization for older environments
  const processed = structuredClone(obj as object) as T;
  replaceParametersInObject(processed, parameters);
  return processed;
}

/**
 * Recursively replace parameter placeholders in an object.
 * @param obj The object to be processed (modified in place)
 * @param parameters The parameter object
 */
function replaceParametersInObject(
  obj: unknown,
  parameters: Record<string, unknown>,
): void {
  if (Array.isArray(obj)) {
    for (let i = 0; i < obj.length; i++) {
      if (typeof obj[i] === "string") {
        obj[i] = replaceParameterInString(obj[i] as string, parameters);
      } else if (typeof obj[i] === "object" && obj[i] !== null) {
        replaceParametersInObject(obj[i], parameters);
      }
    }
  } else if (obj && typeof obj === "object") {
    const objRecord = obj as Record<string, unknown>;
    for (const key in objRecord) {
      if (Object.prototype.hasOwnProperty.call(objRecord, key)) {
        if (typeof objRecord[key] === "string") {
          objRecord[key] = replaceParameterInString(
            objRecord[key] as string,
            parameters,
          );
        } else if (typeof objRecord[key] === "object" && objRecord[key] !== null) {
          replaceParametersInObject(objRecord[key], parameters);
        }
      }
    }
  }
}

/**
 * Replace parameter placeholders in a string.
 *
 * Matches {{parameters.paramName}} pattern and replaces with actual value.
 * Parameter names support: letters, digits, underscores, dots, and hyphens.
 *
 * Examples:
 * - {{parameters.userName}} → "Alice"
 * - {{parameters.user.name}} → "Bob" (nested path support)
 * - {{parameters.my-var}} → "value" (hyphen support)
 *
 * @param str The string to process
 * @param parameters The parameter object
 * @returns The string with placeholders replaced
 */
function replaceParameterInString(
  str: string,
  parameters: Record<string, unknown>,
): string {
  // Support parameter names with letters, digits, underscores, dots, and hyphens
  const regex = /\{\{parameters\.([a-zA-Z0-9_.-]+)\}\}/g;
  return str.replace(regex, (match, paramName: string) => {
    if (parameters[paramName] !== undefined) {
      return String(parameters[paramName]);
    }
    // Keep original if parameter not found
    return match;
  });
}
