/**
 * Current Time Fragment Generator
 *
 * Generates dynamic content based on the current time.
 */

import { wrapSection } from "./utils.js";

/**
 * Generate the current time content.
 */
export function generateCurrentTimeContent(): string {
  const now = new Date();
  return `Current Time: ${now.toISOString()}`;
}

/**
 * Generate the current time period.
 */
export function generateCurrentTimeSection(): string {
  return wrapSection("CURRENT TIME", generateCurrentTimeContent());
}
