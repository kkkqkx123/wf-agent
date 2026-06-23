/**
 * Logger Factory Functions
 * Provides convenient functions to create different types of loggers
 */

import type { Logger, LogLevel, LoggerOptions } from "./types.js";
import { BaseLogger } from "./base-logger.js";

/**
 * Empty Operation Logger
 * Used to disable log output
 */
class NoopLogger implements Logger {
  debug(): void {}
  info(): void {}
  warn(): void {}
  error(): void {}

  child(): Logger {
    return this;
  }

  setLevel(): void {}
  getLevel(): LogLevel {
    return "off";
  }

  isLevelEnabled(): boolean {
    return false;
  }
}

/**
 * Create a logger instance
 * @param options Logger configuration options
 * @returns Logger instance
 */
export function createLogger(options: LoggerOptions = {}): Logger {
  return new BaseLogger(options);
}

/**
 * Create a package-level logger
 * This is the recommended way to create a logger:
 * @param pkg Package name
 * @param options Logger configuration options
 * @returns Logger instance
 */
export function createPackageLogger(
  pkg: string,
  options: Omit<LoggerOptions, "name"> = {},
): Logger {
  const packageOptions: LoggerOptions = {
    ...options,
    name: pkg,
  };

  const packageContext = {
    pkg,
  };

  return new BaseLogger(packageOptions, packageContext);
}

/**
 * Create a default console logger
 * @param level Log level, default is 'info'
 * @returns Logger instance
 */
export function createConsoleLogger(level: LogLevel = "info"): Logger {
  return new BaseLogger({ level });
}

/**
 * Create an empty operation logger
 * Used to disable log output
 * @returns Logger instance
 */
export function createNoopLogger(): Logger {
  return new NoopLogger();
}
