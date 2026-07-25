/**
 * Runtime Mode Detector
 * Centralized detection of execution modes and output formats.
 *
 * Extracted from apps/cli-app/src/utils/mode-detector.ts to eliminate
 * duplication between cli-app and server.
 *
 * Detection priority (highest to lowest):
 *   1. Environment variables (CLI_MODE, HEADLESS, TEST_MODE)
 *      Note: CLI_MODE=test or TEST_MODE=true maps to "test".
 *            TEST_MODE=true was previously equivalent to headless;
 *            it now maps to "test" so foreground/background modes
 *            are not downgraded during testing.
 *   2. Config-provided default (passed via getMode())
 *   3. Hard-coded default ("interactive")
 *
 * Usage:
 *   import { getMode, isHeadless, isJsonMode } from "@wf-agent/runtime/mode";
 *
 *   if (isHeadless()) { /* use JSON output *&#47; }
 *   const mode = getMode(); // { mode, outputFormat, colorEnabled, ... }
 */

import type { ExecutionMode, OutputFormat } from "./types.js";
import { ExecutionModeEnvVars } from "./types.js";

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
  /** Whether running in interactive mode (shortcut) */
  isInteractive: boolean;
  /** Whether running in test mode (shortcut) */
  isTest: boolean;
}

/**
 * Environment variable names for mode detection
 */
const ENV = ExecutionModeEnvVars;

/**
 * Detect execution mode from environment, with optional config fallback.
 * Priority: env vars > config fallback > hard-coded default ("interactive")
 *
 * Detection logic:
 *   CLI_MODE=test       -> test       (highest priority)
 *   CLI_MODE=headless   -> headless
 *   TEST_MODE=true      -> test       (was headless before reform)
 *   HEADLESS=true       -> headless
 *   configFallback      -> as specified (only when no env var matches)
 *   default             -> interactive
 *
 * @param configFallback Optional mode from app configuration as fallback
 */
function detectMode(configFallback?: ExecutionMode): ExecutionMode {
  const cliMode = process.env[ENV.CLI_MODE];

  // CLI_MODE=test takes highest priority
  if (cliMode === "test") {
    return "test";
  }

  // CLI_MODE=headless (or legacy "programmatic") -> headless
  if (cliMode === "headless" || cliMode === "programmatic") {
    return "headless";
  }

  // TEST_MODE=true now maps to "test" (not headless)
  // This allows foreground/background modes to work in tests
  // without being downgraded to blocking.
  if (process.env[ENV.TEST_MODE] === "true") {
    return "test";
  }

  // HEADLESS=true -> headless
  if (process.env[ENV.HEADLESS] === "true") {
    return "headless";
  }

  // Fallback to config value if provided
  if (configFallback === "headless" || configFallback === "test") {
    return configFallback;
  }

  return "interactive";
}

/**
 * Detect output format from environment
 * @param mode Pre-detected execution mode (to avoid redundant detection)
 */
function detectOutputFormat(mode?: ExecutionMode): OutputFormat {
  const format = process.env[ENV.OUTPUT_FORMAT] as OutputFormat | undefined;
  if (format === "json" || format === "silent" || format === "text") return format;

  const currentMode = mode ?? detectMode();

  // Headless mode defaults to json output
  if (currentMode === "headless") return "json";

  // Test mode defaults to text output (easier for test assertions)
  if (currentMode === "test") return "text";

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
 *
 * @param configFallback Optional mode from app configuration as fallback
 *        (only used when no env var is set)
 */
export function getMode(configFallback?: ExecutionMode): ModeDetectionResult {
  if (cacheValid && cachedResult) {
    return cachedResult;
  }

  const mode = detectMode(configFallback);
  const result: ModeDetectionResult = {
    mode,
    outputFormat: detectOutputFormat(mode),
    colorEnabled: detectColorEnabled(),
    isHeadless: mode === "headless",
    isInteractive: mode === "interactive",
    isTest: mode === "test",
  };

  cachedResult = result;
  cacheValid = true;
  return result;
}

/**
 * Get the current output format
 */
export function getOutputFormat(): OutputFormat {
  return getMode().outputFormat;
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
 * Quick check: interactive mode
 */
export function isInteractive(): boolean {
  return getMode().isInteractive;
}

/**
 * Quick check: test mode
 */
export function isTest(): boolean {
  return getMode().isTest;
}
