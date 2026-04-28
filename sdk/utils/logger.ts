/**
 * SDK Logger Tool
 * Provides a unified logging interface with separate namespaces for sdk, graph and agent modules
 *
 * Design Principles:
 * - Lazy initialization to allow CLI-APP to configure before first use
 * - Supports runtime stream replacement for output redirection
 * - Registration to the global registry supports unified control
 * - Uses Proxy for lazy initialization without module-load side effects
 * - Separate namespaces for sdk, graph and agent modules for fine-grained control
 *
 * Environment Variable Naming Convention (Layered Architecture):
 * - GLOBAL_*: Global settings affecting all modules
 * - SDK_*: SDK module specific settings
 * - SDK_*_GRAPH: SDK Graph submodule settings
 * - SDK_*_AGENT: SDK Agent submodule settings
 */

import { createPackageLogger, registerLogger } from "@wf-agent/common-utils";
import type { Logger, LogStream, LogLevel } from "@wf-agent/common-utils";

// Global logger instances - initialized lazily on first access
let sdkLoggerInstance: Logger | null = null;
let graphLoggerInstance: Logger | null = null;
let agentLoggerInstance: Logger | null = null;

// Flags to track if loggers have been initialized
let isSDKInitialized = false;
let isGraphInitialized = false;
let isAgentInitialized = false;

// Pending configuration to be applied when loggers are created
let pendingSDKConfig: {
  level?: LogLevel;
  stream?: LogStream;
} | null = null;

let pendingGraphConfig: {
  level?: LogLevel;
  stream?: LogStream;
} | null = null;

let pendingAgentConfig: {
  level?: LogLevel;
  stream?: LogStream;
} | null = null;

// ============================================
// Environment Variable Names (Layered Architecture)
// ============================================

const ENV_VARS = {
  // Global settings
  GLOBAL_LOG_LEVEL: "GLOBAL_LOG_LEVEL",

  // SDK module specific
  SDK_LOG_LEVEL: "SDK_LOG_LEVEL",
  SDK_LOG_LEVEL_GRAPH: "SDK_LOG_LEVEL_GRAPH",
  SDK_LOG_LEVEL_AGENT: "SDK_LOG_LEVEL_AGENT",
} as const;

// ============================================
// Helper Functions
// ============================================

/**
 * Get log level from environment variable with fallback chain
 * Priority: primaryKey > globalKey > defaultLevel
 */
function getLogLevelFromEnv(
  primaryKey: string,
  globalKey: string = ENV_VARS.GLOBAL_LOG_LEVEL,
  defaultLevel: LogLevel = "info",
): LogLevel {
  const value = process.env[primaryKey] as LogLevel | undefined;
  if (value) return value;
  const globalValue = process.env[globalKey] as LogLevel | undefined;
  if (globalValue) return globalValue;
  return defaultLevel;
}

/**
 * Get log level for graph module from environment variables
 * Priority: SDK_LOG_LEVEL_GRAPH > SDK_LOG_LEVEL > GLOBAL_LOG_LEVEL > info
 */
function getGraphLogLevel(): LogLevel {
  return getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL_GRAPH, ENV_VARS.GLOBAL_LOG_LEVEL, "info");
}

/**
 * Get log level for agent module from environment variables
 * Priority: SDK_LOG_LEVEL_AGENT > SDK_LOG_LEVEL > GLOBAL_LOG_LEVEL > info
 */
function getAgentLogLevel(): LogLevel {
  return getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL_AGENT, ENV_VARS.GLOBAL_LOG_LEVEL, "info");
}

/**
 * Get log level for SDK core module from environment variables
 * Priority: SDK_LOG_LEVEL > GLOBAL_LOG_LEVEL > info
 */
function getSDKLogLevel(): LogLevel {
  return getLogLevelFromEnv(ENV_VARS.SDK_LOG_LEVEL, ENV_VARS.GLOBAL_LOG_LEVEL, "info");
}

// ============================================
// Logger Creation Functions
// ============================================

/**
 * Create the graph logger instance
 * Internal function to actually create the logger
 */
function createGraphLoggerInstance(level: LogLevel, stream?: LogStream): Logger {
  const instance = createPackageLogger("sdk.graph", {
    level,
    stream,
    timestamp: true,
  });

  // Register with the global registry
  registerLogger("sdk.graph", instance);

  return instance;
}

/**
 * Create the agent logger instance
 * Internal function to actually create the logger
 */
function createAgentLoggerInstance(level: LogLevel, stream?: LogStream): Logger {
  const instance = createPackageLogger("sdk.agent", {
    level,
    stream,
    timestamp: true,
  });

  // Register with the global registry
  registerLogger("sdk.agent", instance);

  return instance;
}

/**
 * Initialize the graph logger explicitly
 * Call this before using any graph logger methods to ensure proper configuration
 * @param config Optional configuration, uses environment variables if not provided
 */
export function initializeGraphLogger(config?: { level?: LogLevel; stream?: LogStream }): Logger {
  if (isGraphInitialized && graphLoggerInstance) {
    return graphLoggerInstance;
  }

  const level = config?.level ?? pendingGraphConfig?.level ?? getGraphLogLevel();
  const stream = config?.stream ?? pendingGraphConfig?.stream;

  graphLoggerInstance = createGraphLoggerInstance(level, stream);
  isGraphInitialized = true;

  return graphLoggerInstance;
}

/**
 * Initialize the agent logger explicitly
 * Call this before using any agent logger methods to ensure proper configuration
 * @param config Optional configuration, uses environment variables if not provided
 */
export function initializeAgentLogger(config?: { level?: LogLevel; stream?: LogStream }): Logger {
  if (isAgentInitialized && agentLoggerInstance) {
    return agentLoggerInstance;
  }

  const level = config?.level ?? pendingAgentConfig?.level ?? getAgentLogLevel();
  const stream = config?.stream ?? pendingAgentConfig?.stream;

  agentLoggerInstance = createAgentLoggerInstance(level, stream);
  isAgentInitialized = true;

  return agentLoggerInstance;
}

/**
 * Get or create the graph logger instance
 * Uses lazy initialization - only creates logger on first access
 */
function getGraphLoggerInstance(): Logger {
  if (!graphLoggerInstance) {
    graphLoggerInstance = initializeGraphLogger();
  }
  return graphLoggerInstance;
}

/**
 * Get or create the agent logger instance
 * Uses lazy initialization - only creates logger on first access
 */
function getAgentLoggerInstance(): Logger {
  if (!agentLoggerInstance) {
    agentLoggerInstance = initializeAgentLogger();
  }
  return agentLoggerInstance;
}

// ============================================
// SDK Logger Creation Functions
// ============================================

/**
 * Create the SDK logger instance
 * Internal function to actually create the logger
 */
function createSDKLoggerInstance(level: LogLevel, stream?: LogStream): Logger {
  const instance = createPackageLogger("sdk", {
    level,
    stream,
    timestamp: true,
  });

  registerLogger("sdk", instance);

  return instance;
}

/**
 * Initialize the SDK logger explicitly
 * @param config Optional configuration, uses environment variables if not provided
 */
export function initializeSDKLogger(config?: { level?: LogLevel; stream?: LogStream }): Logger {
  if (isSDKInitialized && sdkLoggerInstance) {
    return sdkLoggerInstance;
  }

  const level = config?.level ?? pendingSDKConfig?.level ?? getSDKLogLevel();
  const stream = config?.stream ?? pendingSDKConfig?.stream;

  sdkLoggerInstance = createSDKLoggerInstance(level, stream);
  isSDKInitialized = true;

  return sdkLoggerInstance;
}

/**
 * Get or create the SDK logger instance
 * Uses lazy initialization - only creates logger on first access
 */
function getSDKLoggerInstance(): Logger {
  if (!sdkLoggerInstance) {
    sdkLoggerInstance = initializeSDKLogger();
  }
  return sdkLoggerInstance;
}

/**
 * Create a module-level logger for SDK core module
 * @param moduleName Module name
 * @returns An instance of the module-level Logger
 */
export function createSDKModuleLogger(moduleName: string): Logger {
  return getSDKLoggerInstance().child(moduleName);
}

/**
 * SDK core module package-level logger
 * Uses Proxy for lazy initialization - no side effects on module load
 * First access triggers initialization, subsequent accesses use cached instance
 */
export const sdkLogger: Logger = new Proxy({} as Logger, {
  get(_, prop: string | symbol) {
    const instance = getSDKLoggerInstance();
    const value = (instance as unknown as Record<string | symbol, unknown>)[prop];

    if (typeof value === "function") {
      return (value as (...args: unknown[]) => unknown).bind(instance);
    }

    return value;
  },
});

/**
 * Configure the SDK loggers
 * Must be called by CLI-APP before any SDK usage to ensure proper configuration
 * @param config Configuration options
 */
export function configureSDKLogger(config: {
  level?: LogLevel;
  sdkLevel?: LogLevel;
  graphLevel?: LogLevel;
  agentLevel?: LogLevel;
  stream?: LogStream;
}): void {
  pendingSDKConfig = {
    level: config.sdkLevel ?? config.level,
    stream: config.stream,
  };
  pendingGraphConfig = {
    level: config.graphLevel ?? config.level,
    stream: config.stream,
  };
  pendingAgentConfig = {
    level: config.agentLevel ?? config.level,
    stream: config.stream,
  };

  // If loggers already exist, update them
  if (sdkLoggerInstance) {
    sdkLoggerInstance.setLevel(config.sdkLevel ?? config.level ?? "info");
    if (config.stream && sdkLoggerInstance.setStream) {
      sdkLoggerInstance.setStream(config.stream);
    }
  }

  if (graphLoggerInstance) {
    graphLoggerInstance.setLevel(config.graphLevel ?? config.level ?? "info");
    if (config.stream && graphLoggerInstance.setStream) {
      graphLoggerInstance.setStream(config.stream);
    }
  }

  if (agentLoggerInstance) {
    agentLoggerInstance.setLevel(config.agentLevel ?? config.level ?? "info");
    if (config.stream && agentLoggerInstance.setStream) {
      agentLoggerInstance.setStream(config.stream);
    }
  }
}

/**
 * Create a module-level logger for graph module
 * @param moduleName Module name
 * @returns An instance of the module-level Logger
 */
export function createGraphModuleLogger(moduleName: string): Logger {
  return getGraphLoggerInstance().child(moduleName);
}

/**
 * Create a module-level logger for agent module
 * @param moduleName Module name
 * @returns An instance of the module-level Logger
 */
export function createAgentModuleLogger(moduleName: string): Logger {
  return getAgentLoggerInstance().child(moduleName);
}

/**
 * Graph module package-level logger
 * Uses Proxy for lazy initialization - no side effects on module load
 * First access triggers initialization, subsequent accesses use cached instance
 */
export const graphLogger: Logger = new Proxy({} as Logger, {
  get(_, prop: string | symbol) {
    const instance = getGraphLoggerInstance();
    const value = (instance as unknown as Record<string | symbol, unknown>)[prop];

    if (typeof value === "function") {
      return (value as (...args: unknown[]) => unknown).bind(instance);
    }

    return value;
  },
});

/**
 * Agent module package-level logger
 * Uses Proxy for lazy initialization - no side effects on module load
 * First access triggers initialization, subsequent accesses use cached instance
 */
export const agentLogger: Logger = new Proxy({} as Logger, {
  get(_, prop: string | symbol) {
    const instance = getAgentLoggerInstance();
    const value = (instance as unknown as Record<string | symbol, unknown>)[prop];

    if (typeof value === "function") {
      return (value as (...args: unknown[]) => unknown).bind(instance);
    }

    return value;
  },
});
