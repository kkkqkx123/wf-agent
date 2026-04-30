/**
 * Line number utilities for content formatting and validation.
 * 
 * Provides functionality for:
 * - Stripping line numbers from formatted content
 * - Validating if content has line numbers
 * - Smart truncation preserving context from both start and end
 */

/**
 * Checks if every line in the content has line numbers prefixed (e.g., "123 | content").
 * Line numbers must be followed by a single pipe character (not double pipes).
 * 
 * @param content - The content to check
 * @returns True if every non-empty line has line numbers
 */
export function everyLineHasLineNumbers(content: string): boolean {
  const lines = content.split(/\r?\n/); // Handles both CRLF and LF line endings
  return lines.length > 0 && lines.every((line) => /^\s*\d+\s+\|(?!\|)/.test(line));
}

/**
 * Strips line numbers from content while preserving the actual content.
 * 
 * @param content - The content to process
 * @param aggressive - When false (default): Only strips lines with clear number patterns like "123 | content"
 *                     When true: Uses a more lenient pattern that also matches lines with just a pipe character,
 *                     which can be useful when LLMs don't perfectly format the line numbers in diffs
 * @returns The content with line numbers removed
 */
export function stripLineNumbers(content: string, aggressive: boolean = false): string {
  // Split into lines to handle each line individually
  const lines = content.split(/\r?\n/);

  // Process each line
  const processedLines = lines.map((line) => {
    // Match line number pattern and capture everything after the pipe
    const match = aggressive
      ? line.match(/^\s*(?:\d+\s)?\|\s(.*)$/)
      : line.match(/^\s*\d+\s+\|(?!\|)\s?(.*)$/);
    return match ? match[1] : line;
  });

  // Join back with original line endings (CRLF or LF)
  const lineEnding = content.includes("\r\n") ? "\r\n" : "\n";
  let result = processedLines.join(lineEnding);

  // Preserve trailing newline if present in original content
  if (content.endsWith(lineEnding)) {
    if (!result.endsWith(lineEnding)) {
      result += lineEnding;
    }
  }

  return result;
}

/**
 * Truncates multi-line output while preserving context from both the beginning and end.
 * When truncation is needed, it keeps 20% of the lines from the start and 80% from the end,
 * with a clear indicator of how many lines were omitted in between.
 * 
 * IMPORTANT: Character limit takes precedence over line limit. This is because:
 * 1. Character limit provides a hard cap on memory usage and context window consumption
 * 2. A single line with millions of characters could bypass line limits and cause issues
 * 3. Character limit ensures consistent behavior regardless of line structure
 * 
 * When both limits are specified:
 * - If content exceeds character limit, character-based truncation is applied (regardless of line count)
 * - If content is within character limit but exceeds line limit, line-based truncation is applied
 * - This prevents edge cases where extremely long lines could consume excessive resources
 * 
 * @param content - The multi-line string to truncate
 * @param lineLimit - Optional maximum number of lines to keep. If not provided or 0, no line limit is applied
 * @param characterLimit - Optional maximum number of characters to keep. If not provided or 0, no character limit is applied
 * @returns The truncated string with an indicator of omitted content, or the original content if no truncation needed
 * 
 * @example
 * // With 10 line limit on 25 lines of content:
 * // - Keeps first 2 lines (20% of 10)
 * // - Keeps last 8 lines (80% of 10)
 * // - Adds "[...15 lines omitted...]" in between
 * 
 * @example
 * // With character limit on long single line:
 * // - Keeps first 20% of characters
 * // - Keeps last 80% of characters
 * // - Adds "[...X characters omitted...]" in between
 * 
 * @example
 * // Character limit takes precedence:
 * // content = "A".repeat(50000) + "\n" + "B".repeat(50000) // 2 lines, 100,002 chars
 * // truncateOutput(content, 10, 40000) // Uses character limit, not line limit
 * // Result: First ~8000 chars + "[...60002 characters omitted...]" + Last ~32000 chars
 */
export function truncateOutput(
  content: string,
  lineLimit?: number,
  characterLimit?: number
): string {
  // If no limits are specified, return original content
  if (!lineLimit && !characterLimit) {
    return content;
  }

  // Character limit takes priority over line limit
  if (characterLimit && content.length > characterLimit) {
    const beforeLimit = Math.floor(characterLimit * 0.2); // 20% of characters before
    const afterLimit = characterLimit - beforeLimit; // remaining 80% after

    const startSection = content.slice(0, beforeLimit);
    const endSection = content.slice(-afterLimit);
    const omittedChars = content.length - characterLimit;

    return startSection + `\n[...${omittedChars} characters omitted...]\n` + endSection;
  }

  // If character limit is not exceeded or not specified, check line limit
  if (!lineLimit) {
    return content;
  }

  // Count total lines
  let totalLines = 0;
  let pos = -1;
  while ((pos = content.indexOf("\n", pos + 1)) !== -1) {
    totalLines++;
  }
  totalLines++; // Account for last line without newline

  if (totalLines <= lineLimit) {
    return content;
  }

  const beforeLimit = Math.floor(lineLimit * 0.2); // 20% of lines before
  const afterLimit = lineLimit - beforeLimit; // remaining 80% after

  // Find start section end position
  let startEndPos = -1;
  let lineCount = 0;
  pos = 0;
  while (lineCount < beforeLimit && (pos = content.indexOf("\n", pos)) !== -1) {
    startEndPos = pos;
    lineCount++;
    pos++;
  }

  // Find end section start position
  let endStartPos = content.length;
  lineCount = 0;
  pos = content.length;
  while (lineCount < afterLimit && (pos = content.lastIndexOf("\n", pos - 1)) !== -1) {
    endStartPos = pos + 1; // Start after the newline
    lineCount++;
  }

  const omittedLines = totalLines - lineLimit;
  const startSection = content.slice(0, startEndPos + 1);
  const endSection = content.slice(endStartPos);
  return startSection + `\n[...${omittedLines} lines omitted...]\n\n` + endSection;
}
