/**
 * User Context Builder
 *
 * Builds variable user context that changes frequently during execution:
 * - TODO lists and task status
 * - Pinned files content
 * - Workspace file updates
 * - Runtime state changes
 *
 * These fragments are appended to the last user message to avoid
 * invalidating the KV cache on the system message.
 *
 * Note: This module is a stub for future implementation.
 * Currently returns empty string as the primary use case is
 * injecting stable system context.
 */

import type { DynamicRuntimeContext } from "@wf-agent/types";

/**
 * Build user context content
 *
 * Generates variable content for last user message.
 * This allows frequent updates without invalidating system message cache.
 *
 * @param context Runtime context with TODO list, pinned files, etc.
 * @returns User context content string
 */
export async function buildUserContextContent(
  _context?: DynamicRuntimeContext,
): Promise<string> {
  // TODO: Implement when user-level context features are needed
  // - TODO list injection
  // - Pinned files context
  // - Workspace state updates

  return "";
}
