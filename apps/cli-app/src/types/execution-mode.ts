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
 * Log level type
 */
export type LogLevel = "debug" | "verbose" | "info" | "warn" | "error";

/**
 * Exit code configuration
 */
export interface ExitCodeConfig {
  success: number;
  error: number;
  validationError: number;
  timeout: number;
  cancelled: number;
}

/**
 * Execution mode configuration
 */
export interface ExecutionModeConfig {
  /** Execution mode */
  mode: ExecutionMode;

  /** Output format */
  outputFormat: OutputFormat;

  /** Whether to auto exit (headless mode) */
  autoExit: boolean;

  /** Timeout in milliseconds */
  timeout: number;

  /** Log level */
  logLevel: LogLevel;

  /** Log file path */
  logFile?: string;

  /** Whether to disable colored output */
  noColor: boolean;

  /** Exit code mapping */
  exitCodes: ExitCodeConfig;
}

/**
 * Default exit code configuration
 */
export const defaultExitCodes: ExitCodeConfig = {
  success: 0,
  error: 1,
  validationError: 2,
  timeout: 124,
  cancelled: 130,
};

/**
 * Default execution mode configuration
 */
export const defaultExecutionConfig: ExecutionModeConfig = {
  mode: "interactive",
  outputFormat: "text",
  autoExit: false,
  timeout: 30000,
  logLevel: "info",
  noColor: false,
  exitCodes: defaultExitCodes,
};

/**
 * Execution mode detection result
 */
export interface ExecutionModeDetection {
  mode: ExecutionMode;
  source: "env" | "cli" | "default";
}

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
