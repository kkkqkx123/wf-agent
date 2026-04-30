/**
 * Diff statistics computation utilities.
 */

/**
 * Compute statistics from a unified diff patch.
 * 
 * @param unifiedDiff - The unified diff string
 * @returns Statistics about additions and deletions, or undefined if parsing fails
 */
export function computeDiffStats(unifiedDiff: string): { additions: number; deletions: number } | undefined {
  if (!unifiedDiff || typeof unifiedDiff !== "string") {
    return undefined;
  }

  const lines = unifiedDiff.split("\n");
  let additions = 0;
  let deletions = 0;

  for (const line of lines) {
    // Skip hunk headers and file headers
    if (line.startsWith("@@") || line.startsWith("---") || line.startsWith("+++")) {
      continue;
    }

    // Count additions (lines starting with + but not +++ )
    if (line.startsWith("+") && !line.startsWith("+++")) {
      additions++;
    }

    // Count deletions (lines starting with - but not --- )
    if (line.startsWith("-") && !line.startsWith("---")) {
      deletions++;
    }
  }

  return { additions, deletions };
}

/**
 * Sanitize a unified diff by removing unnecessary metadata.
 * Keeps only the essential diff content.
 * 
 * @param diff - The raw unified diff
 * @returns Sanitized diff string
 */
export function sanitizeUnifiedDiff(diff: string): string {
  if (!diff || typeof diff !== "string") {
    return diff;
  }

  const lines = diff.split("\n");
  const sanitized: string[] = [];

  for (const line of lines) {
    // Keep hunk headers, context lines, additions, and deletions
    if (
      line.startsWith("@@") ||
      line.startsWith(" ") ||
      line.startsWith("+") ||
      line.startsWith("-") ||
      line === ""
    ) {
      sanitized.push(line);
    }
    // Skip --- and +++ file headers
  }

  return sanitized.join("\n");
}
