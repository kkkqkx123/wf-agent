/**
 * Runtime Mode Types
 * Shared execution mode types for Modular Agent Framework applications.
 *
 * Extracted from apps/cli-app/src/types/execution-mode.ts to eliminate
 * duplication between cli-app and server.
 */

/**
 * Execution mode type
 * - interactive: Full terminal UI with user interaction
 * - headless: Automated mode with JSON output, no user prompts
 * - programmatic: Controlled by another program, structured output
 */
export type ExecutionMode = "interactive" | "headless" | "programmatic";

/**
 * Output format type
 * - text: Human-readable text output (may include ANSI colors)
 * - json: Structured JSON output for programmatic consumption
 * - silent: Suppress all output
 */
export type OutputFormat = "text" | "json" | "silent";

/**
 * Environment variable mapping for execution mode
 */
export const ExecutionModeEnvVars = {
  /** Primary mode configuration */
  CLI_MODE: "CLI_MODE",
  /** Legacy headless flag */
  HEADLESS: "HEADLESS",
  /** Legacy test mode flag */
  TEST_MODE: "TEST_MODE",
  /** Output format */
  OUTPUT_FORMAT: "CLI_OUTPUT_FORMAT",
  /** Log level */
  LOG_LEVEL: "CLI_LOG_LEVEL",
  /** Log file path */
  LOG_FILE: "CLI_LOG_FILE",
  /** Disable color */
  NO_COLOR: "NO_COLOR",
} as const;