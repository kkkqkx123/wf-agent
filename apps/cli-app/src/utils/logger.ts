/**
 * CLI Logger
 * CLI-specific logging setup, routing through CLIOutput's single log stream.
 *
 * The core logging logic (initLogger, initSDKLogger) is provided by
 * @wf-agent/runtime/logger to eliminate duplication between apps.
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - CLI_*: CLI module specific settings
 * - SDK_*: SDK module specific settings
 * - SDK_*_GRAPH: SDK Graph submodule settings
 * - SDK_*_AGENT: SDK Agent submodule settings
 */

import type { LogStream, LogEntry } from "@wf-agent/common-utils";
import { getOutput } from "./output.js";
import {
  initLogger as runtimeInitLogger,
  initSDKLogger as runtimeInitSDKLogger,
  initAllLoggers as runtimeInitAllLoggers,
} from "@wf-agent/runtime/logger";
import type { LoggerOptions as RuntimeLoggerOptions } from "@wf-agent/runtime/logger";

// ============================================
// Types
// ============================================

export interface LoggerOptions extends RuntimeLoggerOptions {
  // CLI-specific extensions (if any) go here
}

// ============================================
// Environment Variable Names (Layered Architecture)
// ============================================

const ENV_VARS = {
  // Global settings (affect all modules)
  /** Global log level for all modules */
  GLOBAL_LOG_LEVEL: "GLOBAL_LOG_LEVEL",

  // CLI module specific
  /** CLI module log level */
  CLI_LOG_LEVEL: "CLI_LOG_LEVEL",

  // SDK module specific
  /** SDK base log level */
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
  /** Disable SDK logs */
  SDK_DISABLE_LOGS: "SDK_DISABLE_LOGS",
} as const;

// ============================================
// LogStream Adapter
// ============================================

/**
 * Adapter that routes LogStream writes through CLIOutput's single log stream.
 * This eliminates dual-file-write conflicts between the logger system and CLIOutput.
 */
class OutputLogStreamAdapter implements LogStream {
  write(entry: LogEntry): void {
    const output = getOutput();
    const context = entry.context as Record<string, unknown> | undefined;
    if (entry.timestamp) {
      output.log(entry.level, entry.message, {
        ...context,
        _ts: entry.timestamp,
      });
    } else {
      output.log(entry.level, entry.message, context);
    }
  }
}

const outputLogStream: LogStream = new OutputLogStreamAdapter();

// ============================================
// Initialization Functions
// ============================================

/**
 * Initialize CLI Logger
 * Should be called at program startup, before SDK initialization
 */
export function initLogger(options: LoggerOptions = {}): void {
  runtimeInitLogger(options, outputLogStream, "cli-app");
}

/**
 * Initialize SDK Logger
 * Should be called before SDK initialization
 */
export function initSDKLogger(options: LoggerOptions = {}): void {
  runtimeInitSDKLogger(options, outputLogStream);
}

/**
 * Initialize all Loggers (CLI + SDK)
 *
 * IMPORTANT: Call this function BEFORE importing or using any SDK modules
 * to ensure proper logger configuration.
 *
 * @param options Logger configuration options
 */
export function initAllLoggers(options: LoggerOptions = {}): void {
  runtimeInitAllLoggers(options, outputLogStream, "cli-app");
}

/**
 * Re-export environment variable names for documentation purposes
 */
export { ENV_VARS };
