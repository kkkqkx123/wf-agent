/**
 * Fixed File Fragment Generator
 */

import type { PinnedFileItem } from "@wf-agent/types";
import { wrapSection } from "./utils.js";

/**
 * Generate fixed file content
 */
export function generatePinnedFilesContent(
  pinnedFiles?: PinnedFileItem[],
  sectionTitle?: string,
): string {
  if (!pinnedFiles || pinnedFiles.length === 0) {
    return "";
  }

  const lines: string[] = ["Pinned files:"];
  for (const file of pinnedFiles) {
    if (file && file.path) {
      lines.push(`  - ${file.path}`);
    }
  }

  return wrapSection(sectionTitle || "PINNED FILES CONTENT", lines.join("\n"));
}
