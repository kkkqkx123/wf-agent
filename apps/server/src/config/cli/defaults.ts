/**
 * Server Configuration Defaults
 * Default configuration values for Server application.
 * Extends the base defaults from @wf-agent/runtime.
 */

import type { CLIConfig } from "./types.js";
import { DEFAULT_CONFIG as BASE_DEFAULTS } from "@wf-agent/runtime";

/**
 * Default Configuration
 * Merges base defaults with server-specific defaults.
 */
export const DEFAULT_CONFIG: CLIConfig = {
  ...BASE_DEFAULTS,
  outputFormat: "json",
  maxConcurrentExecutions: 10,
  storage: {
    type: "sqlite",
    sqlite: {
      dbPath: "./storage/server.db",
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
    logFilePattern: "server-{date}.log",
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
