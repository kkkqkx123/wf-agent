/**
 * Route Helpers
 *
 * Utility functions for route handlers
 */

/**
 * Safely get string from request params/query
 */
export function getSafeParam(
  value: string | string[] | Record<string, any> | undefined
): string | undefined {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value[0];
  return undefined;
}

/**
 * Safely parse integer from query param
 */
export function getIntParam(
  value: string | string[] | Record<string, any> | undefined,
  defaultValue: number = 0
): number {
  const str = getSafeParam(value);
  if (!str) return defaultValue;
  const parsed = parseInt(str, 10);
  return isNaN(parsed) ? defaultValue : parsed;
}
