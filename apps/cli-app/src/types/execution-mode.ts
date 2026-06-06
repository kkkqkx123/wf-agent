/**
 * Execution Mode Types
 * Type definitions for CLI execution modes
 */

/**
 * Execution mode type
 */
export type ExecutionMode = "interactive" | "headless" | "programmatic";

/**
 * Output format type
 */
export type OutputFormat = "text" | "json" | "silent";

/**
 * Environment variable mapping
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