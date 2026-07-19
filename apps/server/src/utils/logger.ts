/**
 * Server Logger
 * Server-specific logging setup, routing through the server's output system.
 *
 * The core logging logic (initLogger, initSDKLogger) is provided by
 * @wf-agent/runtime/logger to eliminate duplication between apps.
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - SERVER_*: Server module specific settings
 * - SDK_*: SDK module specific settings
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
  // Server-specific extensions (if any) go here
}

// ============================================
// Environment Variable Names (Layered Architecture)
// ============================================

const ENV_VARS = {
  /** Global log level for all modules */
  GLOBAL_LOG_LEVEL: "GLOBAL_LOG_LEVEL",
  /** Server module log level */
  SERVER_LOG_LEVEL: "SERVER_LOG_LEVEL",
  /** SDK base log level */
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
  /** Disable SDK logs */
  SDK_DISABLE_LOGS: "SDK_DISABLE_LOGS",
} as const;

// ============================================
// LogStream Adapter
// ============================================

/**
 * Adapter that routes LogStream writes through the server's output system.
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
 * Initialize Server Logger
 */
export function initLogger(options: LoggerOptions = {}): void {
  runtimeInitLogger(options, outputLogStream, "server");
}

/**
 * Initialize SDK Logger
 */
export function initSDKLogger(options: LoggerOptions = {}): void {
  runtimeInitSDKLogger(options, outputLogStream);
}

/**
 * Initialize all loggers
 */
export function initAllLoggers(options: LoggerOptions = {}): void {
  runtimeInitAllLoggers(options, outputLogStream, "server");
}

/**
 * Re-export environment variable names
 */
export { ENV_VARS };
