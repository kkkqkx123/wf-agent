/**
 * Workspace File Tree Fragment Generator
 */

import { wrapSection } from "./utils.js";

/**
 * Generate the workspace file tree content.
 */
export function generateWorkspaceFilesContent(
  fileTree?: string,
  _maxDepth?: number,
  _ignorePatterns?: string[],
): string {
  if (!fileTree) return "";
  return wrapSection(
    "WORKSPACE FILES",
    `The following is a list of files in the current workspace:\n\n${fileTree}`,
  );
}
