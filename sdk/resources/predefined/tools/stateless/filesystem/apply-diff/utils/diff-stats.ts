/**
 * Diff statistics computation utilities for apply_diff tool.
 * Computes addition and deletion counts from SEARCH/REPLACE format diffs.
 */

/**
 * Compute statistics from SEARCH/REPLACE format diff.
 * Counts additions and deletions by analyzing REPLACE vs SEARCH blocks.
 * 
 * @param searchReplaceDiff - The SEARCH/REPLACE format diff string
 * @returns Statistics about additions and deletions, or undefined if parsing fails
 */
export function computeSearchReplaceStats(searchReplaceDiff: string): { additions: number; deletions: number } | undefined {
  if (!searchReplaceDiff || typeof searchReplaceDiff !== "string") {
    return undefined;
  }

  // Parse SEARCH/REPLACE blocks
  const blockPattern = /<<<<<<< SEARCH[\s\S]*?=======([\s\S]*?)>>>>>>> REPLACE/g;
  const matches = [...searchReplaceDiff.matchAll(blockPattern)];

  if (matches.length === 0) {
    return undefined;
  }

  let additions = 0;
  let deletions = 0;

  for (const match of matches) {
    const replaceContent = match[1] || "";
    const lines = replaceContent.split("\n");
    
    // Count non-empty lines in REPLACE section as additions
    for (const line of lines) {
      if (line.trim() !== "") {
        additions++;
      }
    }
  }

  // For deletions, we need to count SEARCH content lines
  const searchPattern = /<<<<<<< SEARCH[\s\S]*?=======/g;
  const searchBlocks = [...searchReplaceDiff.matchAll(/<<<<<<< SEARCH([\s\S]*?)=======/g)];
  
  for (const match of searchBlocks) {
    const searchContent = match[1] || "";
    const lines = searchContent.split("\n");
    
    // Count non-empty lines in SEARCH section as deletions
    for (const line of lines) {
      if (line.trim() !== "") {
        deletions++;
      }
    }
  }

  return { additions, deletions };
}


