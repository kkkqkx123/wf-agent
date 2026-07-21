/**
 * Shared formatter utilities
 */

import { getFormatter, Formatter } from "../formatter.js";

/**
 * Truncate a string with ellipsis if it exceeds maxLen.
 */
export function truncate(str: string, maxLen: number): string {
  if (str.length <= maxLen) return str;
  return str.substring(0, maxLen) + "...";
}

/**
 * Shorten an ID for display, returning "N/A" when undefined.
 */
export function shortId(id: string | undefined, len: number = 8): string {
  return id ? id.substring(0, len) : "N/A";
}

/**
 * Standardized empty-list message.
 */
export function emptyMsg(entity: string): string {
  return `No ${entity} found.`;
}

/**
 * Boilerplate eliminator: get the formatter, optionally JSON-dump data
 * in verbose mode, otherwise delegate to formatFn.
 */
export function formatWith<T>(
  data: T,
  options: { verbose?: boolean } | undefined,
  formatFn: (formatter: Formatter) => string,
): string {
  const formatter = getFormatter();
  if (options?.verbose) {
    return formatter.json(data);
  }
  return formatFn(formatter);
}