/**
 * @wf-agent/runtime - Shared Runtime Package
 *
 * Provides unified SDK bootstrap, storage management, logger initialization,
 * lifecycle management, and shared adapter base classes for Modular Agent
 * Framework applications.
 *
 * This package eliminates the duplicated initialization/integration code
 * between apps/cli-app and apps/server.
 */

// Bootstrap - Unified SDK initialization
export { createAppSDK } from "./bootstrap/create-app-sdk.js";
export type { AppSDKOptions, AppSDKResult } from "./bootstrap/create-app-sdk.js";

// Storage - Unified storage adapter management
export { StorageManager } from "./storage/storage-manager.js";

// Logger - Unified logger initialization
export { initLogger, initSDKLogger, initAllLoggers, ENV_VARS } from "./logger/init-logger.js";
export type { LoggerOptions } from "./logger/init-logger.js";

// Lifecycle - Graceful shutdown and signal handling
export { gracefulShutdown, registerShutdownHandlers } from "./lifecycle/graceful-shutdown.js";

// Adapters - Shared base class for app adapters
export { BaseAppAdapter, AdapterError } from "./adapters/base-adapter.js";

// Config - Shared configuration types and utilities
export type { RuntimeStorageConfig, AppConfig } from "./config/types.js";
export { parseConfigContent, loadConfigFromFile, loadConfigFile, tryLoadConfigFile, readConfigFile, createAppConfigLoader } from "./config/loader.js";
export type { ConfigFormat, LoadedConfig, AppConfigLoaderOptions, AppConfigLoader } from "./config/loader.js";
export { DEFAULT_CONFIG } from "./config/defaults.js";
export { ConfigAccessor } from "./config/accessor.js";
export { ConfigValidator, validateConfig, validateConfigOrThrow } from "./config/validator.js";

// Mode - Shared execution mode detection
export type { ExecutionMode, OutputFormat, ModeDetectionResult } from "./mode/index.js";
export { ExecutionModeEnvVars, getMode, getOutputFormat, isJsonMode, isSilentMode, isHeadless, isProgrammatic, isInteractive, invalidateModeCache } from "./mode/index.js";