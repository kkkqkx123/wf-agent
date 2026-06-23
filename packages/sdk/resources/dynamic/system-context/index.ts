/**
 * System Context Module (stable)
 *
 * Exports stable system context that rarely changes during execution.
 * Safe to cache as these values change infrequently.
 */

export { buildSystemContextPrompt } from "./builder.js";

// Re-export fragments for use within this module
export * from "./fragments/index.js";
