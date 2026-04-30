/**
 * Text normalization utilities for handling HTML entities and special characters.
 */

/**
 * Unescape HTML entities in a string.
 * Converts HTML entities like &lt;, &gt;, &amp;, &quot; back to their original characters.
 * 
 * @param text - The text containing HTML entities
 * @returns The text with HTML entities unescaped
 */
export function unescapeHtmlEntities(text: string): string {
  if (!text || typeof text !== "string") {
    return text;
  }

  return text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&#x27;/g, "'")
    .replace(/&#x2F;/g, "/")
    .replace(/&#x60;/g, "`");
}

/**
 * Escape HTML entities in a string.
 * Converts special characters to their HTML entity equivalents.
 * 
 * @param text - The text to escape
 * @returns The text with special characters escaped as HTML entities
 */
export function escapeHtmlEntities(text: string): string {
  if (!text || typeof text !== "string") {
    return text;
  }

  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
