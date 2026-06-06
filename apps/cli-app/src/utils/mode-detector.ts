/**
 * Mode Detector
 * Centralized detection of execution modes and output formats.
 * Consolidates mode detection logic previously scattered across
 * exit-manager.ts, output.ts, and execution-mode.ts.
 */

import type { ExecutionMode, OutputFormat } from "../types/execution-mode.js";
import { ExecutionModeEnvVars } from "../types/execution-mode.js";

/**
 * Mode Detector result
 */
export interface ModeDetectionResult {
  /** Current execution mode */
  mode: ExecutionMode;
  /** Current output format */
  outputFormat: OutputFormat;
  /** Whether ANSI color is enabled */
  colorEnabled: boolean;
  /** Whether running in headless mode (shortcut) */
  isHeadless: boolean;
  /** Whether running in programmatic mode (shortcut) */
  isProgrammatic: boolean;
  /** Whether running in interactive mode (shortcut) */
  isInteractive: boolean;
}

/**
 * Environment variable names for mode detection
 */
const ENV = ExecutionModeEnvVars;

/**
 * Detect execution mode from environment
 */
function detectMode(): ExecutionMode {
  const cliMode = process.env[ENV.CLI_MODE];
  if (cliMode === "programmatic") return "programmatic";
  if (
    cliMode === "headless" ||
    process.env[ENV.HEADLESS] === "true" ||
    process.env[ENV.TEST_MODE] === "true"
  ) {
    return "headless";
  }
  return "interactive";
}

/**
 * Detect output format from environment
 */
function detectOutputFormat(): OutputFormat {
  const format = process.env[ENV.OUTPUT_FORMAT] as OutputFormat | undefined;
  if (format === "json" || format === "silent") return format;

  // Headless mode defaults to json output
  if (detectMode() === "headless") return "json";

  return "text";
}

/**
 * Detect whether ANSI color is supported
 */
function detectColorEnabled(): boolean {
  if (process.env[ENV.NO_COLOR] !== undefined) return false;
  return process.stdout.isTTY === true;
}

// ============================================
// Global detector instance (stateless, safe singleton)
// ============================================

let cachedResult: ModeDetectionResult | null = null;
let cacheValid = false;

/**
 * Invalidate the cached detection result.
 * Useful for testing or when environment changes at runtime.
 */
export function invalidateModeCache(): void {
  cacheValid = false;
}

/**
 * Get the current mode detection result.
 * Results are cached until invalidateModeCache() is called.
 */
export function getMode(): ModeDetectionResult {
  if (cacheValid && cachedResult) {
    return cachedResult;
  }

  const mode = detectMode();
  const result: ModeDetectionResult = {
    mode,
    outputFormat: detectOutputFormat(),
    colorEnabled: detectColorEnabled(),
    isHeadless: mode === "headless",
    isProgrammatic: mode === "programmatic",
    isInteractive: mode === "interactive",
  };

  cachedResult = result;
  cacheValid = true;
  return result;
}

/**
 * Check if running in JSON output mode
 */
export function isJsonMode(): boolean {
  return getMode().outputFormat === "json";
}

/**
 * Check if running in silent mode
 */
export function isSilentMode(): boolean {
  return getMode().outputFormat === "silent";
}

/**
 * Quick check: headless mode
 */
export function isHeadless(): boolean {
  return getMode().isHeadless;
}

/**
 * Quick check: programmatic mode
 */
export function isProgrammatic(): boolean {
  return getMode().isProgrammatic;
}

/**
 * Quick check: interactive mode
 */
export function isInteractive(): boolean {
  return getMode().isInteractive;
}
