/**
 * Platform-aware signal handling utilities
 * 
 * Provides cross-platform signal registration that accounts for
 * differences between Windows and Unix-like systems.
 */

/**
 * Supported shutdown signals by platform
 */
export interface PlatformSignals {
  /** Signals available on current platform */
  available: string[];
  /** Platform name */
  platform: NodeJS.Platform;
}

/**
 * Signal handler function type
 */
export type SignalHandler = (signal: string) => Promise<void> | void;

/**
 * Platform signal handler interface
 */
export interface PlatformSignalHandler {
  /**
   * Register a signal handler
   * @param handler Function to call when signal is received
   */
  register(handler: SignalHandler): void;

  /**
   * Unregister all signal handlers
   */
  unregister(): void;

  /**
   * Get information about supported signals
   */
  getPlatformInfo(): PlatformSignals;
}

/**
 * Get platform-specific signal information
 */
export function getPlatformSignals(): PlatformSignals {
  const isWindows = process.platform === 'win32';

  if (isWindows) {
    return {
      platform: 'win32',
      // Windows only supports these signals via emulation
      available: ['SIGINT', 'SIGBREAK', 'SIGHUP'],
    };
  } else {
    return {
      platform: process.platform,
      // Unix-like systems support standard POSIX signals
      available: ['SIGTERM', 'SIGINT', 'SIGHUP'],
    };
  }
}

/**
 * Create a platform-aware signal handler
 * 
 * Automatically registers appropriate signal handlers based on the current platform.
 * On Windows, only SIGINT, SIGBREAK, and SIGHUP are supported.
 * On Unix, SIGTERM, SIGINT, and SIGHUP are supported.
 * 
 * @example
 * ```typescript
 * const handler = createPlatformSignalHandler();
 * handler.register(async (signal) => {
 *   console.log(`Received ${signal}, performing cleanup...`);
 *   await cleanup();
 * });
 * ```
 */
export function createPlatformSignalHandler(): PlatformSignalHandler {
  const platformInfo = getPlatformSignals();
  const registeredHandlers: Array<[string, SignalHandler]> = [];

  return {
    register(handler: SignalHandler) {
      // Clear any existing handlers
      this.unregister();

      // Register handlers for each supported signal
      for (const signal of platformInfo.available) {
        const listener = async () => {
          try {
            await handler(signal);
          } catch {
            // Error handling in signal handlers
          }
        };

        process.on(signal as NodeJS.Signals, listener);
        registeredHandlers.push([signal, listener]);
      }
    },

    unregister() {
      for (const [signal, listener] of registeredHandlers) {
        process.removeListener(signal as NodeJS.Signals, listener);
      }
      registeredHandlers.length = 0;
    },

    getPlatformInfo() {
      return platformInfo;
    },
  };
}

/**
 * Check if a signal is supported on the current platform
 * @param signal Signal name to check
 * @returns true if the signal can be received on this platform
 */
export function isSignalSupported(signal: string): boolean {
  const platformInfo = getPlatformSignals();
  return platformInfo.available.includes(signal);
}

/**
 * Get a human-readable description of signal support
 */
export function getSignalSupportDescription(): string {
  const info = getPlatformSignals();
  
  if (info.platform === 'win32') {
    return (
      `Windows platform detected. Supported signals: ${info.available.join(', ')}. ` +
      `Note: SIGTERM is NOT supported on Windows. Use SIGINT (Ctrl+C) or SIGBREAK (Ctrl+Break) instead.`
    );
  } else {
    return (
      `Unix-like platform (${info.platform}) detected. Supported signals: ${info.available.join(', ')}. ` +
      `All standard POSIX signals are available.`
    );
  }
}
