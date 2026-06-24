/**
 * Check if a resource is disabled based on allowList/blockList options.
 *
 * Rules:
 * - If allowList is non-empty, only resources in the list are enabled
 * - If blockList is non-empty, resources in the list are disabled
 * - Otherwise, the resource is enabled
 */
export function isResourceDisabled(
  resourceId: string,
  options?: { allowList?: string[]; blockList?: string[] },
): boolean {
  if (!options) return false;

  if (options.allowList && options.allowList.length > 0) {
    return !options.allowList.includes(resourceId);
  }

  if (options.blockList && options.blockList.length > 0) {
    return options.blockList.includes(resourceId);
  }

  return false;
}
