/**
 * Graceful shutdown controller
 * 
 * Provides a unified interface for implementing graceful shutdown
 * across different platforms with proper timeout handling.
 */

import { createPlatformSignalHandler, type PlatformSignalHandler } from './platform-signals.js';

/**
 * Configuration for graceful shutdown
 */
export interface GracefulShutdownOptions {
  /**
   * Maximum time to wait for shutdown operations (milliseconds)
   * Default: 15000 (15 seconds)
   * Note: On Windows with SIGHUP (console close), maximum effective time is ~10 seconds
   */
  timeoutMs?: number;

  /**
   * Shutdown handler function
   * Called when a shutdown signal is received
   * @param signal The signal that triggered shutdown
   * @param deadline Date by which shutdown must complete
   */
  onShutdown: (signal: string, deadline: Date) => Promise<void>;

  /**
   * Whether to enable the shutdown controller
   * Default: true
   */
  enabled?: boolean;
}

/**
 * Graceful shutdown controller
 * 
 * Manages process shutdown signals and ensures cleanup operations
 * complete within the specified timeout.
 * 
 * @example
 * ```typescript
 * const controller = new GracefulShutdownController({
 *   timeoutMs: 10000,
 *   onShutdown: async (signal, deadline) => {
 *     console.log(`Shutting down due to ${signal}`);
 *     await saveState();
 *     await closeConnections();
 *   }
 * });
 * 
 * controller.start();
 * ```
 */
export class GracefulShutdownController {
  private signalHandler: PlatformSignalHandler;
  private isShuttingDown: boolean = false;
  private options: Required<GracefulShutdownOptions>;

  constructor(options: GracefulShutdownOptions) {
    this.signalHandler = createPlatformSignalHandler();
    this.options = {
      timeoutMs: options.timeoutMs ?? 15000,
      onShutdown: options.onShutdown,
      enabled: options.enabled ?? true,
    };
  }

  /**
   * Start listening for shutdown signals
   */
  start(): void {
    if (!this.options.enabled) {
      console.log('Graceful shutdown controller is disabled');
      return;
    }

    const platformInfo = this.signalHandler.getPlatformInfo();
    console.log(
      `Starting graceful shutdown controller on ${platformInfo.platform}. ` +
      `Supported signals: ${platformInfo.available.join(', ')}`
    );

    this.signalHandler.register(async (signal) => {
      await this.handleShutdown(signal);
    });
  }

  /**
   * Stop listening for shutdown signals
   */
  stop(): void {
    this.signalHandler.unregister();
  }

  /**
   * Check if shutdown is currently in progress
   */
  isShutdownInProgress(): boolean {
    return this.isShuttingDown;
  }

  /**
   * Manually trigger shutdown (for testing or programmatic use)
   * @param signal Optional signal name (default: 'SIGTERM')
   */
  async triggerShutdown(signal: string = 'SIGTERM'): Promise<void> {
    await this.handleShutdown(signal);
  }

  /**
   * Handle shutdown signal
   */
  private async handleShutdown(signal: string): Promise<void> {
    if (this.isShuttingDown) {
      console.warn(`Shutdown already in progress, ignoring ${signal}`);
      return;
    }

    this.isShuttingDown = true;
    const startTime = Date.now();
    const deadline = new Date(startTime + this.options.timeoutMs);

    console.log(`Received ${signal}, initiating graceful shutdown...`);
    console.log(`Shutdown deadline: ${deadline.toISOString()} (${this.options.timeoutMs}ms timeout)`);

    try {
      // Execute shutdown handler with timeout
      await Promise.race([
        this.options.onShutdown(signal, deadline),
        new Promise<never>((_, reject) => {
          setTimeout(() => {
            reject(new Error(`Graceful shutdown timed out after ${this.options.timeoutMs}ms`));
          }, this.options.timeoutMs);
        }),
      ]);

      const duration = Date.now() - startTime;
      console.log(`Graceful shutdown completed successfully in ${duration}ms`);
      process.exit(0);
    } catch (error) {
      const duration = Date.now() - startTime;
      console.error(`Graceful shutdown failed after ${duration}ms:`, error);
      process.exit(1);
    }
  }
}

/**
 * Create a simple shutdown handler that executes multiple cleanup functions
 * 
 * @param cleanupFunctions Array of cleanup functions to execute in order
 * @returns A shutdown handler function
 * 
 * @example
 * ```typescript
 * const controller = new GracefulShutdownController({
 *   timeoutMs: 10000,
 *   onShutdown: createSequentialShutdownHandler([
 *     async () => await checkpointManager.saveAll(),
 *     async () => await database.close(),
 *     async () => await logger.flush(),
 *   ])
 * });
 * ```
 */
export function createSequentialShutdownHandler(
  cleanupFunctions: Array<() => Promise<void>>
): (signal: string, deadline: Date) => Promise<void> {
  return async (signal: string, deadline: Date) => {
    console.log(`Executing ${cleanupFunctions.length} cleanup functions...`);
    
    for (let i = 0; i < cleanupFunctions.length; i++) {
      // Check if we're past the deadline
      if (Date.now() > deadline.getTime()) {
        console.warn(`Shutdown deadline exceeded, skipping remaining ${cleanupFunctions.length - i} cleanup functions`);
        break;
      }

      const func = cleanupFunctions[i];
      try {
        console.log(`Executing cleanup function ${i + 1}/${cleanupFunctions.length}`);
        if (func) {
          await func();
        }
        console.log(`Cleanup function ${i + 1} completed`);
      } catch (error) {
        console.error(`Cleanup function ${i + 1} failed:`, error);
        // Continue with remaining cleanup functions even if one fails
      }
    }
  };
}
