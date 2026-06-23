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

// Circuit Breaker
export {
  CircuitBreaker,
  circuitBreakerDecorator,
  type CircuitBreakerConfig,
  type CircuitMetrics,
  type CircuitState,
} from "./circuit-breaker.js";

// Tool-related helper functions - Moved to @wf-agent/sdk

// File system utilities
export { fileExists, tryLoadJsonFile } from "./file-utils.js";

// Glob pattern matching
export { matchGlobPattern } from "./glob-utils.js";

// Simple ID generation function (only used inside common-utils)
export function generateId(): string {
  return crypto.randomUUID();
}

export * from "./compression/index.js";
export * from "./process/index.js";
