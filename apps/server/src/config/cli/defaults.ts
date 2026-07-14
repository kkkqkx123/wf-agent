/**
 * CLI Configuration Defaults
 * Default configuration values for CLI application.
 */

import type { CLIConfig } from "./types.js";

/**
 * Default Configuration
 */
export const DEFAULT_CONFIG: CLIConfig = {
  defaultTimeout: 30000,
  verbose: false,
  debug: false,
  logLevel: "warn",
  outputFormat: "table",
  maxConcurrentExecutions: 5,
  storage: {
    type: "sqlite",
    sqlite: {
      dbPath: "./storage/cli-app.db",
      enableWAL: true,
      enableLogging: false,
      readonly: false,
      fileMustExist: false,
      timeout: 5000,
      autoVacuum: 'INCREMENTAL',
      journalSizeLimit: 67108864,
    },
  },
  output: {
    dir: "./outputs",
    logFilePattern: "cli-app-{date}.log",
    enableLogTerminal: true,
    enableSDKLogs: true,
    sdkLogLevel: "silent",
  },
  presets: {
    contextCompression: {
      enabled: true,
    },
    predefinedTools: {
      enabled: true,
    },
    predefinedPrompts: {
      enabled: true,
    },
  },
};
