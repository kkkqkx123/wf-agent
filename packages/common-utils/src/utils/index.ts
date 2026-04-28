/**
 * Stateless Tool Classes
 * Export functions directly instead of objects
 */

// Common Tool Functions
export {
  now,
  timestampFromDate,
  timestampToDate,
  timestampToISOString,
  nowWithTimezone,
  diffTimestamp,
  formatDuration,
} from "./timestamp-utils.js";
export { ok, err, tryCatchAsyncWithSignal, all, any, allWithErrors } from "./result-utils.js";

// Tool-related helper functions - Moved to @wf-agent/sdk

// Simple ID generation function (only used inside common-utils)
export function generateId(): string {
  return crypto.randomUUID();
}

export * from "./signal/index.js";
