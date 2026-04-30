/**
 * Terminal output processing utilities.
 * 
 * Provides functionality for:
 * - Processing carriage returns (\r) to simulate real terminal behavior
 * - Processing backspace characters (\b) in terminal output
 * - Applying run-length encoding to compress repeated lines
 */

/**
 * Processes carriage returns (\r) in terminal output to simulate how a real terminal would display content.
 * This function is optimized for performance by using in-place string operations and avoiding memory-intensive
 * operations like split/join.
 * 
 * Key features:
 * 1. Processes output line-by-line to maximize chunk processing
 * 2. Uses string indexes and substring operations instead of arrays
 * 3. Single-pass traversal of the entire input
 * 4. Special handling for multi-byte characters (like emoji) to prevent corruption
 * 5. Replacement of partially overwritten multi-byte characters with spaces
 * 
 * @param input - The terminal output to process
 * @returns The processed terminal output with carriage returns handled
 */
export function processCarriageReturns(input: string): string {
  // Quick check: if no carriage returns, return the original input
  if (input.indexOf("\r") === -1) return input;

  let output = "";
  let i = 0;
  const len = input.length;

  // Single-pass traversal of the entire input
  while (i < len) {
    // Find current line's end position (newline or end of text)
    let lineEnd = input.indexOf("\n", i);
    if (lineEnd === -1) lineEnd = len;

    // Check if current line contains carriage returns
    let crPos = input.indexOf("\r", i);
    if (crPos === -1 || crPos >= lineEnd) {
      // No carriage returns in this line, copy entire line
      output += input.substring(i, lineEnd);
    } else {
      // Line has carriage returns, handle overwrite logic
      let curLine = input.substring(i, crPos);
      curLine = processLineWithCarriageReturns(input, curLine, crPos, lineEnd);
      output += curLine;
    }

    // Add newline character back only if one was originally present
    if (lineEnd < len) output += "\n";

    // Move to next line
    i = lineEnd + 1;
  }

  return output;
}

/**
 * Helper function to process a single line with carriage returns.
 * Handles the overwrite logic for a line that contains one or more carriage returns.
 * 
 * @param input - The original input string
 * @param initialLine - The line content up to the first carriage return
 * @param initialCrPos - The position of the first carriage return in the line
 * @param lineEnd - The position where the line ends
 * @returns The processed line with carriage returns handled
 */
function processLineWithCarriageReturns(
  input: string,
  initialLine: string,
  initialCrPos: number,
  lineEnd: number
): string {
  let curLine = initialLine;
  let crPos = initialCrPos;

  while (crPos < lineEnd) {
    // Find next carriage return or line end
    let nextCrPos = input.indexOf("\r", crPos + 1);
    if (nextCrPos === -1 || nextCrPos >= lineEnd) nextCrPos = lineEnd;

    // Extract segment after carriage return
    let segment = input.substring(crPos + 1, nextCrPos);

    // Skip empty segments
    if (segment !== "") {
      // Determine how to handle overwrite
      if (segment.length >= curLine.length) {
        // Complete overwrite
        curLine = segment;
      } else {
        // Partial overwrite - need to check for multi-byte character boundary issues
        const potentialPartialChar = curLine.charAt(segment.length);
        const segmentLastCharCode = segment.length > 0 ? segment.charCodeAt(segment.length - 1) : 0;
        const partialCharCode = potentialPartialChar.charCodeAt(0);

        // Simplified condition for multi-byte character detection
        if (
          (segmentLastCharCode >= 0xd800 && segmentLastCharCode <= 0xdbff) || // High surrogate at end of segment
          (partialCharCode >= 0xdc00 && partialCharCode <= 0xdfff) || // Low surrogate at overwrite position
          (curLine.length > segment.length + 1 && partialCharCode >= 0xd800 && partialCharCode <= 0xdbff) // High surrogate followed by another character
        ) {
          // If a partially overwritten multi-byte character is detected, replace with space
          const remainPart = curLine.substring(segment.length + 1);
          curLine = segment + " " + remainPart;
        } else {
          // Normal partial overwrite
          curLine = segment + curLine.substring(segment.length);
        }
      }
    }

    crPos = nextCrPos;
  }

  return curLine;
}

/**
 * Processes backspace characters (\b) in terminal output using index operations.
 * Uses indexOf to efficiently locate and handle backspaces.
 * 
 * Technically terminal only moves the cursor and overwrites in-place,
 * but we assume \b is destructive as an optimization which is acceptable
 * for all progress spinner cases and most terminal output cases.
 * 
 * @param input - The terminal output to process
 * @returns The processed output with backspaces handled
 */
export function processBackspaces(input: string): string {
  let output = "";
  let pos = 0;
  let bsPos = input.indexOf("\b");

  while (bsPos !== -1) {
    // Fast path: exclude char before backspace
    output += input.substring(pos, bsPos - 1);

    // Move past backspace
    pos = bsPos + 1;

    // Count consecutive backspaces
    let count = 0;
    while (input[pos] === "\b") {
      count++;
      pos++;
    }

    // Trim output mathematically for consecutive backspaces
    if (count > 0 && output.length > 0) {
      output = output.substring(0, Math.max(0, output.length - count));
    }

    // Find next backspace
    bsPos = input.indexOf("\b", pos);
  }

  // Add remaining content
  if (pos < input.length) {
    output += input.substring(pos);
  }

  return output;
}

/**
 * Applies run-length encoding to compress repeated lines in text.
 * Only compresses when the compression description is shorter than the repeated content.
 * 
 * @param content - The text content to compress
 * @returns The compressed text with run-length encoding applied
 */
export function applyRunLengthEncoding(content: string): string {
  if (!content) {
    return content;
  }

  let result = "";
  let pos = 0;
  let repeatCount = 0;
  let prevLine: string | null = null;

  while (pos < content.length) {
    const nextNewlineIdx = content.indexOf("\n", pos); // Find next newline index
    const currentLine = nextNewlineIdx === -1 ? content.slice(pos) : content.slice(pos, nextNewlineIdx + 1);

    if (prevLine === null) {
      prevLine = currentLine;
    } else if (currentLine === prevLine) {
      repeatCount++;
    } else {
      if (repeatCount > 0) {
        const compressionDesc = `<previous line repeated ${repeatCount} additional times>\n`;
        if (compressionDesc.length < prevLine.length * (repeatCount + 1)) {
          result += prevLine + compressionDesc;
        } else {
          for (let i = 0; i <= repeatCount; i++) {
            result += prevLine;
          }
        }
        repeatCount = 0;
      } else {
        result += prevLine;
      }
      prevLine = currentLine;
    }

    pos = nextNewlineIdx === -1 ? content.length : nextNewlineIdx + 1;
  }

  if (repeatCount > 0 && prevLine !== null) {
    const compressionDesc = `<previous line repeated ${repeatCount} additional times>\n`;
    if (compressionDesc.length < prevLine.length * repeatCount) {
      result += prevLine + compressionDesc;
    } else {
      for (let i = 0; i <= repeatCount; i++) {
        result += prevLine;
      }
    }
  } else if (prevLine !== null) {
    result += prevLine;
  }

  return result;
}
