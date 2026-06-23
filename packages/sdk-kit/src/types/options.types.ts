/**
 * SDK-Kit Configuration Options
 *
 * Provides configuration for core features:
 * - Logging level configuration
 * - Event history management
 *
 * Design principle: Only define configurations that are implemented and applied.
 * Future features should be added when ready for implementation, not speculatively.
 */

/**
 * Logging level configuration
 */
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/**
 * Logging configuration
 */
export interface LoggingConfig {
  /** Logging level (default: 'info') */
  level?: LogLevel;
}

/**
 * Event system configuration
 */
export interface EventsConfig {
  /** Maximum event history size (default: 10000) */
  maxHistorySize?: number;
  /** Enable event history tracking (default: true) */
  enableHistory?: boolean;
}

/**
 * Main SDK-Kit Options Interface
 * Configuration for SDKKit instance with implemented features only
 */
export interface SDKKitOptions {
  /**
   * Logging configuration
   * Controls logging verbosity
   */
  logging?: LoggingConfig;

  /**
   * Event system configuration
   * Controls event tracking and history
   */
  events?: EventsConfig;
}

/**
 * Default SDK-Kit options
 */
export const DEFAULT_SDK_KIT_OPTIONS: Required<SDKKitOptions> = {
  logging: {
    level: 'info',
  },
  events: {
    maxHistorySize: 10000,
    enableHistory: true,
  },
};

/**
 * Merge user options with defaults
 * User options override defaults
 */
export function mergeSDKKitOptions(
  userOptions?: SDKKitOptions
): Required<SDKKitOptions> {
  if (!userOptions) {
    return structuredClone(DEFAULT_SDK_KIT_OPTIONS);
  }

  return {
    logging: {
      level: userOptions.logging?.level ?? DEFAULT_SDK_KIT_OPTIONS.logging.level,
    },
    events: {
      maxHistorySize: userOptions.events?.maxHistorySize ?? DEFAULT_SDK_KIT_OPTIONS.events.maxHistorySize,
      enableHistory: (userOptions.events?.enableHistory !== undefined)
        ? userOptions.events.enableHistory
        : DEFAULT_SDK_KIT_OPTIONS.events.enableHistory,
    },
  };
}
