/**
 * Dynamic Prompt Word Tool Functions
 *
 * Provides general formatting and processing functions
 */

/**
 * Paragraph Wrapper
 * Wrap the content into paragraphs with headings
 */
export function wrapSection(title: string, content: string | null): string {
  if (!content) return "";
  return `====\n\n${title}\n\n${content}`;
}

/**
 * Remove extra blank lines
 * Compress consecutive 3 or more line breaks into 2
 */
export function cleanupEmptyLines(text: string): string {
  return text.replace(/\n{3,}/g, "\n\n").trim();
}
