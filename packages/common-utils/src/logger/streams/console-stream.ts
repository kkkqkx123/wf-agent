/**
 * Console output stream:
 * Supports both JSON and plain formats, as well as colored output.
 */

import type { LogStream, LogEntry, StreamOptions } from "../types.js";

/**
 * ANSI color codes
 */
const COLORS = {
  reset: "\x1b[0m",
  bright: "\x1b[1m",
  dim: "\x1b[2m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
  magenta: "\x1b[35m",
  cyan: "\x1b[36m",
  white: "\x1b[37m",
};

/**
 * Level Color Mapping
 */
const LEVEL_COLORS: Record<string, string> = {
  debug: COLORS.blue,
  info: COLORS.green,
  warn: COLORS.yellow,
  error: COLORS.red,
};

// formatTimestamp function removed - not used in this file

/**
 * Format log entries as plain text.
 */
function formatPretty(entry: LogEntry, pretty: boolean): string {
  const { level, message, timestamp, context, ...rest } = entry;

  const timestampStr = timestamp ? `[${timestamp}] ` : "";
  const levelStr = pretty
    ? `${LEVEL_COLORS[level] || COLORS.white}[${level.toUpperCase()}]${COLORS.reset} `
    : `[${level.toUpperCase()}] `;

  let output = `${timestampStr}${levelStr}${message}`;

  // Add context and additional fields
  const extraData = { ...context, ...rest };
  if (Object.keys(extraData).length > 0) {
    output += ` ${JSON.stringify(extraData)}`;
  }

  return output;
}

/**
 * ConsoleStream class
 */
export class ConsoleStream implements LogStream {
  private json: boolean;
  private timestamp: boolean;
  private pretty: boolean;

  constructor(options: StreamOptions = {}) {
    this.json = options.json ?? false;
    this.timestamp = options.timestamp ?? true;
    this.pretty = options.pretty ?? false;
  }

  /**
   * Write a log entry
   */
  write(entry: LogEntry): void {
    if (this.json) {
      // Output in JSON format
      // eslint-disable-next-line no-console
      console.log(JSON.stringify(entry));
    } else {
      // Output in plain text:
      const formatted = formatPretty(entry, this.pretty);

      // Select the console method based on the level.
      switch (entry.level) {
        case "debug":
          // eslint-disable-next-line no-console
          console.debug(formatted);
          break;
        case "warn":
          // eslint-disable-next-line no-console
          console.warn(formatted);
          break;
        case "error":
          // eslint-disable-next-line no-console
          console.error(formatted);
          break;
        default:
          // eslint-disable-next-line no-console
          console.log(formatted);
      }
    }
  }

  /**
   * Refresh the buffer (the console does not need to be refreshed).
   */
  flush(callback?: () => void): void {
    if (callback) {
      setImmediate(callback);
    }
  }

  /**
   * Terminate the stream (the console does not need to be terminated).
   */
  end(): void {
    // The console stream does not require any special processing.
  }
}

/**
 * Create a console stream
 */
export function createConsoleStream(options: StreamOptions = {}): LogStream {
  return new ConsoleStream(options);
}
