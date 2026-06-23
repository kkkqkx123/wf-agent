/**
 * Tool-related general utility functions
 */

import { createHash } from "crypto";

/**
 * Generate a tool ID
 * @param name The name of the tool
 * @returns A unique ID in the format `${name}_${hash8}`
 */
export function generateToolId(name: string): string {
  const hash = createHash("md5").update(name).digest("hex").slice(0, 8);
  return `${name}_${hash}`;
}

/**
 * Truncate text (keeping beginning and end)
 * @param text The original text
 * @param maxLength The maximum length
 * @returns The truncated text
 */
export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) {
    return text;
  }

  const halfLength = Math.floor(maxLength / 2) - 50;
  const head = text.slice(0, halfLength);
  const tail = text.slice(-halfLength);

  return `${head}\n\n... [Content truncated: ${text.length} chars -> ${maxLength} limit] ...\n\n${tail}`;
}

/**
 * Format line numbers
 * @param lines Array of text lines
 * @param startLine Starting line number (default is 1)
 * @returns Formatted text with line numbers
 */
export function formatLineNumbers(lines: string[], startLine: number = 1): string {
  return lines
    .map((line, index) => `${(startLine + index).toString().padStart(6, " ")}|${line}`)
    .join("\n");
}
