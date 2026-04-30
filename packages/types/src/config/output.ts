/**
 * Output and Logging Configuration Types
 * Unified output and logging configuration types for all applications
 */

/**
 * Log Level
 */
export type LogLevel = "error" | "warn" | "info" | "debug";

/**
 * SDK Log Level
 */
export type SDKLogLevel = "silent" | "error" | "warn" | "info" | "debug";

/**
 * Output Format
 */
export type OutputFormat = "json" | "table" | "plain";

/**
 * Output Configuration
 */
export interface OutputConfig {
  dir: string;
  logFilePattern: string;
  enableLogTerminal: boolean;
  enableSDKLogs: boolean;
  sdkLogLevel: SDKLogLevel;
}
