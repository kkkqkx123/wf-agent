/**
 * SDK Bootstrap Module
 *
 * Handles complete SDK initialization for the Server application.
 * Ensures all resources are properly configured and ready before accepting requests.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import { createSDK } from "@wf-agent/sdk/api";
import { registerAllIndexResolvers } from "@wf-agent/config-processor";
import type { CLIConfig } from "./config/index.js";
import { StorageManager } from "./storage/index.js";
import { getOutput } from "./utils/output.js";
import { initLogger, initSDKLogger } from "./utils/logger.js";

/**
 * Bootstrap SDK instance with complete initialization
 */
export async function bootstrapSDK(config: CLIConfig): Promise<{
  sdk: SDKInstance;
  storageManager: StorageManager;
}> {
  const output = getOutput();

  try {
    output.infoLog("🔧 Initializing SDK...");

    // 1. Initialize TOML parser
    try {
      const { initializeTomlParser } = await import("@wf-agent/sdk/api");
      await initializeTomlParser();
      output.debugLog("✓ TOML parser initialized");
    } catch (_error) {
      output.debugLog("⚠ TOML parser not available, will use JSON/defaults");
    }

    // 2. Initialize logger system
    initLogger({
      verbose: config.verbose,
      debug: config.debug,
      logFile: config.output?.logFilePattern,
      outputDir: config.output?.dir,
      logFilePattern: config.output?.logFilePattern,
      enableLogTerminal: config.output?.enableLogTerminal,
      enableSDKLogs: config.output?.enableSDKLogs,
      sdkLogLevel: config.output?.sdkLogLevel,
    });

    initSDKLogger({
      verbose: config.verbose,
      debug: config.debug,
      logFile: config.output?.logFilePattern,
      outputDir: config.output?.dir,
      logFilePattern: config.output?.logFilePattern,
      enableLogTerminal: config.output?.enableLogTerminal,
      enableSDKLogs: config.output?.enableSDKLogs,
      sdkLogLevel: config.output?.sdkLogLevel,
    });

    // 3. Initialize storage manager
    const storageManager = new StorageManager(config);
    await storageManager.initialize();
    output.debugLog("✓ Storage manager initialized");

    // 4. Create and initialize SDK
    const sdkInstance = createSDK({
      debug: config.debug,
      logging: {
        level:
          config.logLevel ||
          (config.debug ? "debug" : config.verbose ? "info" : "warn"),
      },
      presets: config.presets,
      strictStorage: true,
      ...storageManager.getAllAdapters(),
      defaultTimeout: config.defaultTimeout,
      workflowExecution: {
        maxConcurrentExecutions: config.maxConcurrentExecutions,
      },
      gracefulShutdown: {
        enabled: true,
        timeoutMs: 15000,
      },
      hooks: {
        onBootstrapStart: () => {
          output.debugLog("SDK bootstrap started");
        },
        onBootstrapComplete: () => {
          output.infoLog("✓ SDK bootstrap completed");
        },
        onBootstrapError: (error: Error) => {
          output.errorLog(`✗ SDK bootstrap failed: ${error.message}`);
        },
        onDestroy: async () => {
          output.infoLog("SDK shutting down...");
          await storageManager.close();
          output.infoLog("✓ Storage closed");
        },
      },
    });

    // 5. Wait for SDK ready
    await sdkInstance.waitForReady();
    output.debugLog("✓ SDK ready");

    // 6. Register all index resolvers
    registerAllIndexResolvers();
    output.debugLog("✓ Index resolvers registered");

    output.infoLog("✅ SDK initialization complete");

    return { sdk: sdkInstance, storageManager };
  } catch (error) {
    output.errorLog(
      `SDK bootstrap failed: ${error instanceof Error ? error.message : String(error)}`
    );
    throw error;
  }
}

/**
 * Graceful shutdown handler
 */
export async function gracefulShutdown(
  sdk: SDKInstance,
  _storageManager: { close: () => Promise<void> }
): Promise<void> {
  const output = getOutput();

  try {
    output.infoLog("🛑 Starting graceful shutdown...");

    // Shutdown SDK (check if method exists, SDK API may vary)
    if (sdk && typeof (sdk as any).gracefulShutdown === "function") {
      await (sdk as any).gracefulShutdown(15000);
      output.infoLog("✓ SDK shutdown complete");
    } else {
      output.debugLog("SDK gracefulShutdown method not available, skipping");
    }

    // Close storage
    await _storageManager.close();
    output.infoLog("✓ Storage closed");

    output.infoLog("✅ Graceful shutdown complete");
  } catch (error) {
    output.errorLog(
      `Graceful shutdown error: ${error instanceof Error ? error.message : String(error)}`
    );
    process.exit(1);
  }
}
