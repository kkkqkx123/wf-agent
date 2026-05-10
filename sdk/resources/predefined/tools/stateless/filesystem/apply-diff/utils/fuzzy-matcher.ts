/**
 * Fuzzy matching utilities for apply_diff tool.
 * Based on roo-code's MultiSearchReplaceDiffStrategy implementation.
 */

import { distance } from "fastest-levenshtein";

export const BUFFER_LINES = 40;

/**
 * Calculate similarity between two strings (0.0 to 1.0).
 * Uses normalized Levenshtein distance.
 */
export function getSimilarity(original: string, search: string): number {
  if (search === "") {
    return 0;
  }

  // Normalize smart quotes and special characters
  const normalizedOriginal = normalizeString(original);
  const normalizedSearch = normalizeString(search);

  if (normalizedOriginal === normalizedSearch) {
    return 1;
  }

  const dist = distance(normalizedOriginal, normalizedSearch);
  const maxLength = Math.max(normalizedOriginal.length, normalizedSearch.length);

  return 1 - dist / maxLength;
}

/**
 * Normalize string to handle smart quotes and special characters.
 */
function normalizeString(str: string): string {
  return str
    .replace(/[\u2018\u2019]/g, "'") // Smart single quotes
    .replace(/[\u201C\u201D]/g, '"') // Smart double quotes
    .replace(/\u2013/g, "-") // En dash
    .replace(/\u2014/g, "--") // Em dash
    .replace(/\u00A0/g, " "); // Non-breaking space
}

/**
 * Perform middle-out fuzzy search to find best matching slice.
 *
 * @param lines - Original file lines
 * @param searchChunk - Content to search for
 * @param startIndex - Search range start
 * @param endIndex - Search range end
 * @returns Best match info
 */
export function fuzzySearch(
  lines: string[],
  searchChunk: string,
  startIndex: number,
  endIndex: number,
): {
  bestScore: number;
  bestMatchIndex: number;
  bestMatchContent: string;
} {
  let bestScore = 0;
  let bestMatchIndex = -1;
  let bestMatchContent = "";

  const searchLen = searchChunk.split(/\r?\n/).length;
  const midPoint = Math.floor((startIndex + endIndex) / 2);

  let leftIndex = midPoint;
  let rightIndex = midPoint + 1;

  while (leftIndex >= startIndex || rightIndex <= endIndex - searchLen) {
    // Search left from midpoint
    if (leftIndex >= startIndex) {
      const originalChunk = lines.slice(leftIndex, leftIndex + searchLen).join("\n");
      const similarity = getSimilarity(originalChunk, searchChunk);

      if (similarity > bestScore) {
        bestScore = similarity;
        bestMatchIndex = leftIndex;
        bestMatchContent = originalChunk;
      }
      leftIndex--;
    }

    // Search right from midpoint
    if (rightIndex <= endIndex - searchLen) {
      const originalChunk = lines.slice(rightIndex, rightIndex + searchLen).join("\n");
      const similarity = getSimilarity(originalChunk, searchChunk);

      if (similarity > bestScore) {
        bestScore = similarity;
        bestMatchIndex = rightIndex;
        bestMatchContent = originalChunk;
      }
      rightIndex++;
    }
  }

  return { bestScore, bestMatchIndex, bestMatchContent };
}

/**
 * Preserve indentation when applying replacements.
 * Calculates relative indentation levels and adjusts accordingly.
 */
export function preserveIndentation(
  matchedLines: string[],
  searchLines: string[],
  replaceLines: string[],
): string[] {
  // Get original indentation
  const originalIndents = matchedLines.map((line) => {
    const match = line.match(/^[\t ]*/);
    return match ? match[0] : "";
  });

  // Get search block indentation
  const searchIndents = searchLines.map((line) => {
    const match = line.match(/^[\t ]*/);
    return match ? match[0] : "";
  });

  // Apply replacement with preserved indentation
  return replaceLines.map((line) => {
    const matchedIndent = originalIndents[0] || "";
    const currentIndentMatch = line.match(/^[\t ]*/);
    const currentIndent = currentIndentMatch ? currentIndentMatch[0] : "";
    const searchBaseIndent = searchIndents[0] || "";

    // Calculate relative indentation level
    const searchBaseLevel = searchBaseIndent.length;
    const currentLevel = currentIndent.length;
    const relativeLevel = currentLevel - searchBaseLevel;

    // Adjust indentation
    const finalIndent =
      relativeLevel < 0
        ? matchedIndent.slice(0, Math.max(0, matchedIndent.length + relativeLevel))
        : matchedIndent + currentIndent.slice(searchBaseLevel);

    return finalIndent + line.trim();
  });
}
