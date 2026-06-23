/**
 * Type Conversion Utilities
 *
 * Provides utilities for inferring and converting string values to their typed equivalents.
 */

/**
 * Attempt to convert a string value to its properly typed equivalent.
 *
 * Supports conversion to:
 * - boolean: "true" → true, "false" → false
 * - number: "-123", "45.67", etc.
 * - string: default for unrecognized values
 *
 * @param value The string value to convert
 * @returns The typed value (string, number, or boolean)
 *
 * @example
 * ```ts
 * stringToTypedValue("true")  // → true (boolean)
 * stringToTypedValue("123")   // → 123 (number)
 * stringToTypedValue("hello") // → "hello" (string)
 * ```
 */
export function stringToTypedValue(value: string): string | number | boolean {
  if (value === "true") return true;
  if (value === "false") return false;

  // Check if the value looks like a number (including decimals and negative)
  if (/^-?\d+\.?\d*$/.test(value)) {
    return parseFloat(value);
  }

  return value;
}
