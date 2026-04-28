/**
 * System Message Types
 *
 * System-level messages for startup, shutdown, configuration, and errors.
 */

/**
 * System Message Type
 */
export enum SystemMessageType {
  /** System startup */
  STARTUP = "system.startup",

  /** System shutdown */
  SHUTDOWN = "system.shutdown",

  /** Configuration change */
  CONFIG_CHANGE = "system.config_change",

  /** System error */
  ERROR = "system.error",
}

/**
 * System Startup Data
 */
export interface SystemStartupData {
  /** Application version */
  version: string;

  /** Startup configuration */
  config: Record<string, unknown>;

  /** Startup timestamp */
  startedAt: number;
}

/**
 * System Shutdown Data
 */
export interface SystemShutdownData {
  /** Shutdown reason */
  reason: "normal" | "error" | "signal" | "timeout";

  /** Shutdown message */
  message?: string;

  /** Uptime in milliseconds */
  uptime: number;
}

/**
 * System Config Change Data
 */
export interface SystemConfigChangeData {
  /** Configuration key that changed */
  key: string;

  /** Old value */
  oldValue: unknown;

  /** New value */
  newValue: unknown;

  /** Source of change */
  source: "cli" | "api" | "file" | "internal";
}

/**
 * System Error Data
 */
export interface SystemErrorData {
  /** Error code */
  code: string;

  /** Error message */
  message: string;

  /** Error stack trace */
  stack?: string;

  /** Additional context */
  context?: Record<string, unknown>;
}
