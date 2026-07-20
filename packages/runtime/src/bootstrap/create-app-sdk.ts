/**
 * Runtime Bootstrap Module
 * Unified SDK initialization for Modular Agent Framework applications.
 *
 * Encapsulates the common bootstrap flow shared by cli-app and server:
 * 1. Initialize TOML parser
 * 2. Create and initialize StorageManager
 * 3. Create SDK instance with storage adapters
 * 4. Wait for SDK ready
 * 5. Register index resolvers
 *
 * Usage:
 *   import { createAppSDK } from "@wf-agent/runtime/bootstrap";
 *
 *   const { sdk, storageManager } = await createAppSDK({
 *     storage: { type: "sqlite", sqlite: { dbPath: "./data.db" } },
 *     debug: true,
 *     appName: "my-app",
 *   });
 */

import { createSDK, type SDKInstance } from "@wf-agent/sdk/api";
import { registerAllIndexResolvers } from "@wf-agent/config-processor";
import { StorageManager } from "../storage/storage-manager.js";
import type { RuntimeStorageConfig } from "../config/types.js";
import type { SDKOptions, SDKLifecycleHooks } from "@wf-agent/sdk/api";

// ============================================
// Types
// ============================================

/**
 * Application-level SDK creation options.
 * Combines SDK options with runtime-specific configuration.
 */
export interface AppSDKOptions {
  /** Runtime storage configuration */
  storage?: RuntimeStorageConfig;

  /** Application name (used for default db path, e.g. "cli-app") */
  appName?: string;

  /** Enable debug mode */
  debug?: boolean;

  /** Enable verbose mode */
  verbose?: boolean;

  /** SDK logging configuration */
  logging?: SDKOptions["logging"];

  /** SDK presets configuration */
  presets?: SDKOptions["presets"];

  /** Default timeout for operations */
  defaultTimeout?: number;

  /** Max concurrent workflow executions */
  maxConcurrentExecutions?: number;

  /** Graceful shutdown configuration */
  gracefulShutdown?: SDKOptions["gracefulShutdown"];

  /** SDK lifecycle hooks */
  hooks?: SDKLifecycleHooks & {
    /** Called after bootstrap completes but before SDK waitForReady */
    onStorageReady?: (storageManager: StorageManager) => void | Promise<void>;
    /** Called after SDK is ready and resolvers are registered */
    onReady?: (sdk: SDKInstance) => void | Promise<void>;
  };

  /** Whether to run strict storage mode */
  strictStorage?: boolean;
}

/**
 * Result of createAppSDK
 */
export interface AppSDKResult {
  /** Initialized SDK instance */
  sdk: SDKInstance;
  /** Initialized StorageManager */
  storageManager: StorageManager;
}

// ============================================
// Bootstrap Function
// ============================================

/**
 * Create and initialize a fully configured SDK instance for an application.
 *
 * Handles the complete bootstrap flow:
 * - TOML parser initialization
 * - Storage adapter creation and initialization
 * - SDK instance creation with all adapters
 * - Index resolver registration
 *
 * @param options Application SDK options
 * @returns Initialized SDK instance and StorageManager
 */
export async function createAppSDK(options: AppSDKOptions = {}): Promise<AppSDKResult> {
  // 1. Initialize TOML parser first (required for config loading)
  try {
    const { initializeTomlParser } = await import("@wf-agent/sdk/api");
    await initializeTomlParser();
  } catch (_error) {
    // Continue without TOML parser - will use JSON or defaults
  }

  // 2. Initialize storage manager
  const storageManager = new StorageManager({
    storage: options.storage?.storage,
    appName: options.appName ?? options.storage?.appName ?? "app",
  });
  await storageManager.initialize();

  // Notify caller that storage is ready
  if (options.hooks?.onStorageReady) {
    await options.hooks.onStorageReady(storageManager);
  }

  // Determine if strict storage mode should be enforced.
  // When storage type is "memory" or no storage is configured, strictStorage
  // must be disabled because all adapters will be undefined.
  const storageType = options.storage?.storage?.type;
  const isStorageDisabled = !storageType || storageType === "memory";
  const effectiveStrictStorage = isStorageDisabled ? false : (options.strictStorage ?? true);

  // 3. Initialize the SDK with storage adapters and lifecycle hooks
  const sdk = createSDK({
    debug: options.debug,
    logging: options.logging ?? {
      level: options.debug ? "debug" : options.verbose ? "info" : "warn",
    },
    presets: options.presets,
    strictStorage: effectiveStrictStorage,
    ...storageManager.getAllAdapters(),
    defaultTimeout: options.defaultTimeout,
    workflowExecution: {
      maxConcurrentExecutions: options.maxConcurrentExecutions,
    },
    gracefulShutdown: options.gracefulShutdown ?? {
      enabled: true,
      timeoutMs: 15000,
    },
    hooks: {
      ...options.hooks,
      onDestroy: async () => {
        // Call the custom onDestroy if provided, then close storage
        if (options.hooks?.onDestroy) {
          await options.hooks.onDestroy();
        }
        await storageManager.close();
      },
    },
  });

  // 4. Wait for SDK ready
  await sdk.waitForReady();

  // 5. Register all index resolvers
  registerAllIndexResolvers();

  // Notify caller that SDK is fully ready
  if (options.hooks?.onReady) {
    await options.hooks.onReady(sdk);
  }

  return { sdk, storageManager };
}