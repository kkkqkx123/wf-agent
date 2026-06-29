/**
 * Registry Initialization Manager
 *
 * Manages the lifecycle of all registries with:
 * - Ordered initialization based on dependencies
 * - Error handling and recovery
 * - State tracking
 * - Configuration application
 *
 * Design:
 * - One instance per SDK instance (created during bootstrap)
 * - Coordinates initialization of all registries from GlobalContext
 * - Handles both synchronous and asynchronous initialization
 * - Provides clear initialization status tracking
 */

import type { RegistriesConfig } from "./registry-configuration.js";
import { resolveInitializationOrder } from "./registry-configuration.js";
import type { GlobalContext } from "@sdk/shared/global-context.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

const logger = createContextualLogger({ component: "RegistryInitializationManager" });

/**
 * Registry initialization state
 */
export enum RegistryInitializationState {
  PENDING = "pending",
  INITIALIZING = "initializing",
  INITIALIZED = "initialized",
  FAILED = "failed",
}

/**
 * Registry initialization result
 */
export interface RegistryInitializationResult {
  /** Registry name */
  name: string;

  /** Final state */
  state: RegistryInitializationState;

  /** Duration in milliseconds */
  duration: number;

  /** Error if initialization failed */
  error?: Error;

  /** Number of retry attempts made */
  retryAttempts: number;
}

/**
 * Initialization options
 */
export interface RegistryInitializationOptions {
  /** Whether to continue on initialization errors */
  continueOnError?: boolean;

  /** Maximum retry attempts per registry */
  maxRetries?: number;

  /** Timeout per registry (milliseconds) */
  timeout?: number;

  /** Whether to initialize registries in parallel (where possible) */
  parallelInitialization?: boolean;
}

/**
 * Registry Initialization Manager
 */
export class RegistryInitializationManager {
  /** Track initialization state of each registry */
  private initializationState: Map<string, RegistryInitializationState> = new Map();

  /** Track initialization errors */
  private initializationErrors: Map<string, Error> = new Map();

  /** Track initialization duration */
  private initializationDurations: Map<string, number> = new Map();

  /** Track initialization retry attempts */
  private retryAttempts: Map<string, number> = new Map();

  constructor(
    private globalContext: GlobalContext,
    private registriesConfig?: RegistriesConfig,
    private options: RegistryInitializationOptions = {},
  ) {
    this.options = {
      continueOnError: true,
      maxRetries: 3,
      timeout: 30000, // 30 seconds default
      parallelInitialization: false,
      ...options,
    };
  }

  /**
   * Initialize all registries that need initialization
   * @returns Array of initialization results
   */
  async initializeAll(): Promise<RegistryInitializationResult[]> {
    const results: RegistryInitializationResult[] = [];

    logger.info("Starting registry initialization", {
      continueOnError: this.options.continueOnError,
      maxRetries: this.options.maxRetries,
    });

    try {
      // Get list of registries that support async initialization
      const registriesToInitialize = this.getRegistriesToInitialize();

      if (registriesToInitialize.length === 0) {
        logger.info("No registries require initialization");
        return results;
      }

      // Resolve initialization order considering dependencies
      const initOrder = resolveInitializationOrder(registriesToInitialize);
      logger.info(`Registry initialization order resolved: ${initOrder.join(" -> ")}`);

      // Initialize in order
      for (const registryName of initOrder) {
        const result = await this.initializeRegistry(registryName);
        results.push(result);

        if (!this.options.continueOnError && result.state === RegistryInitializationState.FAILED) {
          logger.error(`Registry initialization failed: ${registryName}`, {
            error: result.error,
          });
          throw new Error(
            `Failed to initialize registry '${registryName}': ${result.error?.message}`,
          );
        }
      }

      // Log summary
      const successful = results.filter((r) => r.state === RegistryInitializationState.INITIALIZED)
        .length;
      const failed = results.filter((r) => r.state === RegistryInitializationState.FAILED).length;

      logger.info("Registry initialization complete", {
        total: results.length,
        successful,
        failed,
        totalDuration: results.reduce((sum, r) => sum + r.duration, 0),
      });
    } catch (error) {
      logger.error("Registry initialization failed", { error });
      throw error;
    }

    return results;
  }

  /**
   * Initialize a single registry
   */
  private async initializeRegistry(registryName: string): Promise<RegistryInitializationResult> {
    const startTime = Date.now();
    const result: RegistryInitializationResult = {
      name: registryName,
      state: RegistryInitializationState.PENDING,
      duration: 0,
      retryAttempts: 0,
    };

    this.initializationState.set(registryName, RegistryInitializationState.INITIALIZING);
    this.retryAttempts.set(registryName, 0);

    try {
      // Get the registry instance from global context
      const registry = this.globalContext.getRegistry(registryName);

      if (!registry) {
        throw new Error(`Registry not found in global context: ${registryName}`);
      }

      // Check if registry has async initialization method
      if (typeof (registry as any).initializeFromStorage === "function") {
        await this.executeWithRetry(
          registryName,
          () => (registry as any).initializeFromStorage(),
        );
      }

      result.state = RegistryInitializationState.INITIALIZED;
      logger.info(`Registry initialized: ${registryName}`, {
        duration: Date.now() - startTime,
      });
    } catch (error) {
      result.state = RegistryInitializationState.FAILED;
      result.error = error instanceof Error ? error : new Error(String(error));
      result.retryAttempts = this.retryAttempts.get(registryName) || 0;

      this.initializationErrors.set(registryName, result.error);
      logger.error(`Failed to initialize registry: ${registryName}`, {
        error: result.error.message,
        retries: result.retryAttempts,
      });
    } finally {
      result.duration = Date.now() - startTime;
      this.initializationDurations.set(registryName, result.duration);
      this.initializationState.set(registryName, result.state);
    }

    return result;
  }

  /**
   * Execute initialization with retry logic
   */
  private async executeWithRetry(registryName: string, fn: () => Promise<void>): Promise<void> {
    let lastError: Error | undefined;
    const maxRetries = this.options.maxRetries || 3;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        // Execute with timeout
        await this.executeWithTimeout(fn);
        return; // Success
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        if (attempt < maxRetries) {
          this.retryAttempts.set(registryName, attempt + 1);
          const backoffDelay = this.calculateBackoffDelay(attempt);
          logger.warn(`Registry initialization retry: ${registryName}`, {
            attempt: attempt + 1,
            maxRetries,
            delay: backoffDelay,
            error: lastError.message,
          });

          await this.delay(backoffDelay);
        }
      }
    }

    throw lastError || new Error("Unknown initialization error");
  }

  /**
   * Execute function with timeout
   */
  private executeWithTimeout(fn: () => Promise<void>): Promise<void> {
    const timeout = this.options.timeout || 30000;

    return Promise.race([
      fn(),
      new Promise<void>((_, reject) =>
        setTimeout(
          () => reject(new Error(`Initialization timeout after ${timeout}ms`)),
          timeout,
        ),
      ),
    ]);
  }

  /**
   * Calculate exponential backoff delay
   */
  private calculateBackoffDelay(attempt: number): number {
    const baseDelay = 100;
    const multiplier = 2;
    return baseDelay * Math.pow(multiplier, attempt);
  }

  /**
   * Delay utility
   */
  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Get list of registries that need initialization
   */
  private getRegistriesToInitialize(): string[] {
    const configKeys: Array<[string, keyof RegistriesConfig]> = [
      ["workflow", "workflows"],
      ["workflowExecution", "workflowExecutions"],
      ["task", "tasks"],
      ["tool", "tools"],
      ["skill", "skills"],
      ["script", "scripts"],
      ["agentLoop", "agentLoops"],
      ["nodeTemplate", "nodeTemplates"],
      ["hookTemplate", "hookTemplates"],
      ["trigger", "triggers"],
      ["agentProfile", "agentProfiles"],
    ];

    const registries: string[] = [];

    // Check which registries have initialization config
    for (const [name, configKey] of configKeys) {
      if (this.registriesConfig?.[configKey]) {
        registries.push(name);
      }
    }

    // Filter to only those with async initialization capabilities
    return registries.filter((name) => {
      const registry = this.globalContext.getRegistry(name);
      if (!registry) {
        logger.warn(`Registry not available in GlobalContext`, { registry: name });
        return false;
      }

      const hasAsyncInit = typeof (registry as any).initializeFromStorage === "function";
      if (!hasAsyncInit) {
        logger.debug(`Registry does not support async initialization`, { registry: name });
      }
      return hasAsyncInit;
    });
  }

  /**
   * Get initialization state of a specific registry
   */
  getRegistryState(registryName: string): RegistryInitializationState {
    return (
      this.initializationState.get(registryName) || RegistryInitializationState.PENDING
    );
  }

  /**
   * Check if all registries are initialized
   */
  areAllInitialized(): boolean {
    if (this.initializationState.size === 0) {
      return true; // No registries to initialize
    }

    return Array.from(this.initializationState.values()).every(
      (state) => state === RegistryInitializationState.INITIALIZED,
    );
  }

  /**
   * Get summary of initialization results
   */
  getInitializationSummary(): {
    totalRegistries: number;
    initializedCount: number;
    failedCount: number;
    totalDuration: number;
    errors: Array<{ registry: string; error: string }>;
  } {
    const states = Array.from(this.initializationState.values());
    const durations = Array.from(this.initializationDurations.values());

    return {
      totalRegistries: states.length,
      initializedCount: states.filter((s) => s === RegistryInitializationState.INITIALIZED)
        .length,
      failedCount: states.filter((s) => s === RegistryInitializationState.FAILED).length,
      totalDuration: durations.reduce((sum, d) => sum + d, 0),
      errors: Array.from(this.initializationErrors.entries()).map(([registry, error]) => ({
        registry,
        error: error.message,
      })),
    };
  }
}
