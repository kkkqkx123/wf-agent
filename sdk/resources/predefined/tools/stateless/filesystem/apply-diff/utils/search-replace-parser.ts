/**
 * Parser for SEARCH/REPLACE diff format.
 */

export interface SearchReplaceBlock {
  startLine?: number;
  endLine?: number;
  searchContent: string;
  replaceContent: string;
}

/**
 * Parse SEARCH/REPLACE blocks from diff content.
 *
 * Supports format:
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

  // Regex to match SEARCH/REPLACE blocks (built as string to avoid escaping issues)
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
    const startLine = match[2] ? parseInt(match[2], 10) : undefined;
    const endLine = match[4] ? parseInt(match[4], 10) : undefined;
    const searchContent = unescapeMarkers(match[6] || "");
    const replaceContent = unescapeMarkers(match[7] || "");

    blocks.push({
      startLine,
      endLine,
      searchContent,
      replaceContent,
    });
  }

  return { blocks };
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
