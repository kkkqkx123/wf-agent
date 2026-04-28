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
  maxConcurrentThreads: 5,
  storage: {
    type: "json",
    json: {
      baseDir: "./storage",
      enableFileLock: false,
      compression: {
        enabled: false,
        algorithm: "gzip",
        threshold: 1024,
      },
    },
    sqlite: {
      dbPath: "./storage/cli-app.db",
      enableWAL: true,
      enableLogging: false,
      readonly: false,
      fileMustExist: false,
      timeout: 5000,
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
