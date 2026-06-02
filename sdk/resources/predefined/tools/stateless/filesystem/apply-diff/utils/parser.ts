/**
 * Parser for SEARCH/REPLACE diff format.
 */

import type { SearchReplaceBlock } from "./types.js";

/**
 * Parse SEARCH/REPLACE blocks from diff content.
 *
 * Supports format:
 * <<<<<<< SEARCH
 * [search content]
 * =======
 * [replace content]
 * >>>>>>> REPLACE
 *
 * With optional hints:
 * <<<<<<< SEARCH
 * # line: 10
 * # context: function authenticate
 * [search content]
 * =======
 * [replace content]
 * >>>>>>> REPLACE
 *
 * Compatible with legacy format:
 * <<<<<<< SEARCH
 * :start_line:10
 * -------
 * [search content]
 * =======
 * [replace content]
 * >>>>>>> REPLACE
 */
export function parseSearchReplaceBlocks(diffContent: string): {
  blocks: SearchReplaceBlock[];
  error?: string;
} {
  const blocks: SearchReplaceBlock[] = [];

  // Regex to match SEARCH/REPLACE blocks
  const pattern =
    '(?:^|\\n)(?<!\\\\)<<<<<<< SEARCH>?\\s*\\n' +
    '((?:\\:start_line:\\s*(\\d+)\\s*\\n))?' +
    '((?:\\:end_line:\\s*(\\d+)\\s*\\n))?' +
    '((?<!\\\\)-------\\s*\\n)?' +
    '([\\s\\S]*?)(?:\\n)?' +
    '(?:(?<=\\n)(?<!\\\\)=======\\s*\\n)' +
    '([\\s\\S]*?)(?:\\n)?' +
    '(?:(?<=\\n)(?<!\\\\)>>>>>>> REPLACE)(?=\\n|$)';

  const blockRegex = new RegExp(pattern, 'g');
  const matches = [...diffContent.matchAll(blockRegex)];

  if (matches.length === 0) {
    return {
      blocks: [],
      error: "Invalid diff format - missing required SEARCH/REPLACE sections",
    };
  }

  for (const match of matches) {
    const searchContent = unescapeMarkers(match[6] || "");
    const replaceContent = unescapeMarkers(match[7] || "");
    const block: SearchReplaceBlock = {
      searchContent,
      replaceContent,
    };

    // Parse :start_line:N (legacy)
    const legacyLine = match[2] ? parseInt(match[2], 10) : undefined;
    if (legacyLine !== undefined) {
      block.startLine = legacyLine;
    }

    // Parse inline hints from search content (new format)
    const searchLines = block.searchContent.split("\n");
    const hints = extractHints(searchLines);
    if (hints.startLine !== undefined) {
      block.startLine = hints.startLine;
    }
    if (hints.contextHint !== undefined) {
      block.contextHint = hints.contextHint;
    }

    // Strip hint lines from search content
    if (hints.linesRemoved > 0) {
      block.searchContent = searchLines.slice(hints.linesRemoved).join("\n");
    }

    blocks.push(block);
  }

  return { blocks };
}

/**
 * Extract hint lines from the beginning of search content.
 * Supports:
 *   # line: N
 *   # context: text
 */
function extractHints(lines: string[]): {
  startLine?: number;
  contextHint?: string;
  linesRemoved: number;
} {
  let startLine: number | undefined;
  let contextHint: string | undefined;
  let linesRemoved = 0;

  for (const line of lines) {
    const trimmed = line.trim();

    const lineMatch = trimmed.match(/^#\s*line:\s*(\d+)$/i);
    if (lineMatch && lineMatch[1]) {
      startLine = parseInt(lineMatch[1], 10);
      linesRemoved++;
      continue;
    }

    const contextMatch = trimmed.match(/^#\s*context:\s*(.+)$/i);
    if (contextMatch && contextMatch[1]) {
      contextHint = contextMatch[1].trim();
      linesRemoved++;
      continue;
    }

    break;
  }

  return { startLine, contextHint, linesRemoved };
}

/**
 * Unescape markers that were escaped in the content.
 */
function unescapeMarkers(content: string): string {
  return content
    .replace(/^\\<<<<<<</gm, "<<<<<<<")
    .replace(/^\\=======/gm, "=======")
    .replace(/^\\>>>>>>>/gm, ">>>>>>>")
    .replace(/^\\-------/gm, "-------")
    .replace(/^\\:end_line:/gm, ":end_line:")
    .replace(/^\\:start_line:/gm, ":start_line:");
}

/**
 * Validate marker sequencing to detect malformed diffs.
 */
export function validateMarkerSequencing(diffContent: string): {
  success: boolean;
  error?: string;
} {
  enum State {
    START,
    AFTER_SEARCH,
    AFTER_SEPARATOR,
  }

  let currentState = State.START;
  let lineNumber = 0;

  const SEARCH_PATTERN = /^<<<<<<< SEARCH>?$/;
  const SEP = "=======";
  const REPLACE = ">>>>>>> REPLACE";

  for (const line of diffContent.split("\n")) {
    lineNumber++;
    const marker = line.trim();

    switch (currentState) {
      case State.START:
        if (marker === SEP || marker === REPLACE) {
          return {
            success: false,
            error: `ERROR: Invalid marker '${marker}' at line ${lineNumber}. Expected SEARCH marker first.`,
          };
        }
        if (SEARCH_PATTERN.test(marker)) {
          currentState = State.AFTER_SEARCH;
        }
        break;

      case State.AFTER_SEARCH:
        if (marker === SEP) {
          currentState = State.AFTER_SEPARATOR;
        } else if (SEARCH_PATTERN.test(marker) || marker === REPLACE) {
          return {
            success: false,
            error: `ERROR: Invalid marker sequence at line ${lineNumber}.`,
          };
        }
        break;

      case State.AFTER_SEPARATOR:
        if (marker === REPLACE) {
          currentState = State.START;
        } else if (marker === SEP || SEARCH_PATTERN.test(marker)) {
          return {
            success: false,
            error: `ERROR: Invalid marker sequence at line ${lineNumber}.`,
          };
        }
        break;
    }
  }

  if (currentState !== State.START) {
    return {
      success: false,
      error: `ERROR: Incomplete SEARCH/REPLACE block. Missing closing marker.`,
    };
  }

  return { success: true };
}
