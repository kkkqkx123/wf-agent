/**
 * SDK Logger Tool
 * Provides a unified logging interface for the SDK module
 *
 * Design Principles:
 * - Lazy initialization to allow CLI-APP to configure before first use
 * - Supports runtime stream replacement for output redirection
 * - Registration to the global registry supports unified control
 * - Uses Proxy for lazy initialization without module-load side effects
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - SDK_*: SDK module specific settings
 */

import { createPackageLogger, registerLogger, getLogLevelFromEnv } from "@wf-agent/common-utils";
import type { Logger, LogStream, LogLevel } from "@wf-agent/common-utils";

// ============================================
// Type Definitions
// ============================================

interface LoggerConfig {
  level?: LogLevel;
  stream?: LogStream;
}

interface LoggerState {
  instance: Logger | null;
  isInitialized: boolean;
  pendingConfig: LoggerConfig | null;
}

// ============================================
// Environment Variable Names
// ============================================

const ENV_VARS = {
  GLOBAL_LOG_LEVEL: "GLOBAL_LOG_LEVEL",
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
} as const;

// ============================================
// Logger State Management
// ============================================

const loggerStates: {
  sdk: LoggerState;
} = {
  sdk: { instance: null, isInitialized: false, pendingConfig: null },
};

let isSDKConfigured = false;

// ============================================
// Helper Functions
// ============================================

/**
 * Get log level from environment variables with fallback chain
 */
function getLogLevelFromEnvChain(
  primaryKey: string,
  globalKey: string = ENV_VARS.GLOBAL_LOG_LEVEL,
  defaultLevel: LogLevel = "info",
): LogLevel {
  return getLogLevelFromEnv(primaryKey, globalKey, defaultLevel);
}

/**
 * Create a logger instance and register it
 */
function createAndRegisterLogger(name: string, level: LogLevel, stream?: LogStream): Logger {
  const instance = createPackageLogger(name, {
    level,
    stream,
    timestamp: true,
  });
  registerLogger(name, instance);
  return instance;
}

/**
 * Apply configuration to a logger (before or after initialization)
 */
function applyLoggerConfig(state: LoggerState, config: LoggerConfig): void {
  state.pendingConfig = config;

  if (state.instance) {
    state.instance.setLevel(config.level ?? "info");
    if (config.stream && state.instance.setStream) {
      state.instance.setStream(config.stream);
    }
  }
}

/**
 * Initialize a logger with lazy loading support
 */
function initializeLogger(
  key: keyof typeof loggerStates,
  loggerName: string,
  envVarKey: string,
  showWarning = false,
): Logger {
  const state = loggerStates[key];

  if (state.isInitialized && state.instance) {
    return state.instance;
  }

  // Show warning for SDK logger if accessed before configuration
  if (showWarning && !isSDKConfigured && process.env["NODE_ENV"] !== "test") {
    process.stderr.write(
      "[SDK Logger] Warning: SDK logger accessed before configureSDKLogger() was called. " +
        "Using environment variables or defaults. Call configureSDKLogger() before SDK initialization.\n",
    );
  }

  const level = state.pendingConfig?.level ?? getLogLevelFromEnvChain(envVarKey);
  const stream = state.pendingConfig?.stream;

  state.instance = createAndRegisterLogger(loggerName, level, stream);
  state.isInitialized = true;

  return state.instance;
}

/**
 * Get or create a logger instance (lazy initialization)
 */
function getLoggerInstance(
  key: keyof typeof loggerStates,
  loggerName: string,
  envVarKey: string,
  showWarning = false,
): Logger {
  const state = loggerStates[key];

  if (!state.instance) {
    state.instance = initializeLogger(key, loggerName, envVarKey, showWarning);
  }

  return state.instance;
}

/**
 * Create a Proxy-based lazy logger
 */
function createLazyLogger(
  key: keyof typeof loggerStates,
  loggerName: string,
  envVarKey: string,
  showWarning = false,
): Logger {
  return new Proxy({} as Logger, {
    get(_, prop: string | symbol) {
      const instance = getLoggerInstance(key, loggerName, envVarKey, showWarning);
      const value = (instance as unknown as Record<string | symbol, unknown>)[prop];

      if (typeof value === "function") {
        return (value as (...args: unknown[]) => unknown).bind(instance);
      }

      return value;
    },
  });
}

// ============================================
// Public API - Initialization Functions
// ============================================

/**
 * Initialize the SDK logger explicitly
 * @param config Optional configuration, uses environment variables if not provided
 */
export function initializeSDKLogger(config?: LoggerConfig): Logger {
  applyLoggerConfig(loggerStates.sdk, config ?? {});
  return initializeLogger("sdk", "sdk", ENV_VARS.SDK_LOG_LEVEL, true);
}

// ============================================
// Public API - Module Logger Factories
// ============================================

/**
 * Create a module-level logger for SDK core module
 * @param moduleName Module name
 * @returns An instance of the module-level Logger
 */
export function createSDKModuleLogger(moduleName: string): Logger {
  return getLoggerInstance("sdk", "sdk", ENV_VARS.SDK_LOG_LEVEL, true).child(moduleName);
}

// ============================================
// Public API - Configuration
// ============================================

/**
 * Configure the SDK logger
 * Must be called by CLI-APP before any SDK usage to ensure proper configuration
 * @param config Configuration options
 */
export function configureSDKLogger(config: { level?: LogLevel; stream?: LogStream }): void {
  isSDKConfigured = true;

  // Apply configuration to SDK logger
  applyLoggerConfig(loggerStates.sdk, {
    level: config.level,
    stream: config.stream,
  });
}

// ============================================
// Public API - Lazy Logger Instances (Proxy-based)
// ============================================

/**
 * SDK core module package-level logger
 * Uses Proxy for lazy initialization - no side effects on module load
 * First access triggers initialization, subsequent accesses use cached instance
 */
export const sdkLogger: Logger = createLazyLogger("sdk", "sdk", ENV_VARS.SDK_LOG_LEVEL, true);
