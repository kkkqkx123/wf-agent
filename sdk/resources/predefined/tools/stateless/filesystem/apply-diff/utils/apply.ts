/**
 * Core logic for applying SEARCH/REPLACE blocks to file content
 */

import type { SearchReplaceBlock, BlockApplyResult } from "./types.js";
import { seekSequence } from "../../../../utils/matcher.js";

/**
 * Preserve indentation when applying replacements.
 * Calculates relative indentation levels and adjusts accordingly.
 */
function preserveIndentation(
  matchedLines: string[],
  searchLines: string[],
  replaceLines: string[],
): string[] {
  // Get original indentation
  const originalIndents = matchedLines.map(line => {
    const match = line.match(/^[\t ]*/);
    return match![0];
  });

  // Get search block indentation
  const searchIndents = searchLines.map(line => {
    const match = line.match(/^[\t ]*/);
    return match![0];
  });

  // Apply replacement with preserved indentation
  return replaceLines.map(line => {
    const matchedIndent = originalIndents[0] ?? "";
    const currentIndentMatch = line.match(/^[\t ]*/);
    const currentIndent = currentIndentMatch ? currentIndentMatch[0] : "";
    const searchBaseIndent = searchIndents[0] ?? "";

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

/**
 * Find context hint position in file for disambiguation.
 * Returns the line index where context hint first appears.
 */
function findContextHint(lines: string[], hint: string): number | null {
  const lowerHint = hint.toLowerCase();
  for (let i = 0; i < lines.length; i++) {
    if (lines[i]!.toLowerCase().includes(lowerHint)) {
      return i;
    }
  }
  return null;
}

/**
 * Apply a single SEARCH/REPLACE block to the content lines.
 */
export function applyBlock(
  resultLines: string[],
  block: SearchReplaceBlock,
  delta: number,
): { success: true; lines: string[]; delta: number } | BlockApplyResult {
  const { searchContent, replaceContent, contextHint } = block;
  const startLine = block.startLine ? block.startLine + delta : 0;

  // Split content into lines
  const searchLines = searchContent === "" ? [] : searchContent.split(/\r?\n/);
  const replaceLines = replaceContent === "" ? [] : replaceContent.split(/\r?\n/);

  // Validate non-empty search
  if (searchLines.length === 0) {
    return {
      success: false,
      error: "Empty search content is not allowed",
    };
  }

  // Determine search bounds
  let searchStartIndex = 0;

  // If startLine provided, use it as the starting point
  if (startLine > 0) {
    searchStartIndex = startLine - 1;
  }

  // If contextHint provided, search near the context first
  let matchIndex: number | null = null;
  if (contextHint) {
    const contextIndex = findContextHint(resultLines, contextHint);
    if (contextIndex !== null) {
      // Search around the context hint (±10 lines)
      const contextStart = Math.max(0, contextIndex - 10);
      const contextEnd = Math.min(resultLines.length, contextIndex + searchLines.length + 10);
      const contextLines = resultLines.slice(0, contextEnd);
      matchIndex = seekSequence(contextLines, searchLines, contextStart);
    }
  }

  // Fall back to full search if context hint search failed
  if (matchIndex === null) {
    matchIndex = seekSequence(resultLines, searchLines, searchStartIndex);
  }

  // Check if match is found
  if (matchIndex === null) {
    const lineRange = startLine ? ` near line: ${startLine}` : "";
    const context =
      startLine > 0
        ? resultLines.slice(Math.max(0, startLine - 3), Math.min(resultLines.length, startLine + 4))
        : resultLines.slice(0, 7);
    const contextStr = context
      .map((l, i) => {
        const lineNum = startLine > 0 ? startLine - 3 + i : i + 1;
        return `${lineNum}: ${l}`;
      })
      .join("\n");
    const contextLabel = contextHint ? ` near "${contextHint}"` : "";
    return {
      success: false,
      error: `No match found${lineRange}${contextLabel}.\nNearby lines:\n${contextStr}`,
    };
  }

  // Get matched lines and preserve indentation
  const matchedLines = resultLines.slice(matchIndex, matchIndex + searchLines.length);

  const indentedReplaceLines = preserveIndentation(matchedLines, searchLines, replaceLines);

  // Apply replacement
  const beforeMatch = resultLines.slice(0, matchIndex);
  const afterMatch = resultLines.slice(matchIndex + searchLines.length);

  const newLines = [...beforeMatch, ...indentedReplaceLines, ...afterMatch];
  const newDelta = delta - matchedLines.length + replaceLines.length;

  return {
    success: true,
    lines: newLines,
    delta: newDelta,
  };
}
