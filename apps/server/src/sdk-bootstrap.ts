/**
 * SDK Bootstrap Module
 *
 * Handles complete SDK initialization for the Server application.
 * Uses @wf-agent/runtime/bootstrap for unified SDK creation.
 *
 * Graceful shutdown is delegated to @wf-agent/runtime/lifecycle
 * to eliminate duplication between apps.
 *
 * Ensures all resources are properly configured and ready before accepting requests.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import { createAppSDK } from "@wf-agent/runtime/bootstrap";
import { gracefulShutdown } from "@wf-agent/runtime/lifecycle";
import type { CLIConfig } from "./config/index.js";
import { getOutput } from "./utils/output.js";
import { initLogger, initSDKLogger, type LoggerOptions } from "./utils/logger.js";

/**
 * Bootstrap SDK instance with complete initialization
 */
export async function bootstrapSDK(config: CLIConfig): Promise<{
  sdk: SDKInstance;
  storageManager: import("@wf-agent/runtime/storage").StorageManager;
}> {
  const output = getOutput();

  try {
    output.infoLog("Initializing SDK...");

    // 1. Initialize logger system
    const loggerOptions: LoggerOptions = {
      verbose: config.verbose,
      debug: config.debug,
      logFile: config.output?.logFilePattern,
      outputDir: config.output?.dir,
      logFilePattern: config.output?.logFilePattern,
      enableLogTerminal: config.output?.enableLogTerminal,
      enableSDKLogs: config.output?.enableSDKLogs,
      sdkLogLevel: config.output?.sdkLogLevel,
    };

    initLogger(loggerOptions);
    initSDKLogger(loggerOptions);

    // 2. Create SDK and storage via runtime
    const result = await createAppSDK({
      appName: "server",
      debug: config.debug,
      verbose: config.verbose,
      logging: {
        level:
          config.logLevel ||
          (config.debug ? "debug" : config.verbose ? "info" : "warn"),
      },
      storage: {
        storage: config.storage,
        appName: "server",
      },
      presets: config.presets,
      strictStorage: true,
      defaultTimeout: config.defaultTimeout,
      maxConcurrentExecutions: config.maxConcurrentExecutions,
      hooks: {
        onBootstrapStart: () => {
          output.debugLog("SDK bootstrap started");
        },
        onBootstrapComplete: () => {
          output.infoLog("SDK bootstrap completed");
        },
        onBootstrapError: (error: Error) => {
          output.errorLog(`SDK bootstrap failed: ${error.message}`);
        },
      },
    });

    output.infoLog("SDK initialization complete");

    return result;
  } catch (error) {
    output.errorLog(
      `SDK bootstrap failed: ${error instanceof Error ? error.message : String(error)}`
    );
    throw error;
  }
}

export { gracefulShutdown };
