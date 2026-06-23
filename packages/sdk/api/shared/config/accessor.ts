/**
 * Configuration Accessor
 *
 * Provides a generic configuration accessor pattern for SDK layer.
 * Application-specific accessors (e.g., CLIConfigAccessor) should extend
 * or compose this base functionality.
 *
 * Design principles:
 * - Generic and reusable across different config types
 * - Simple get/set/reset pattern
 * - No I/O operations (pure accessor)
 */

/**
 * Configuration Accessor Interface
 *
 * Generic interface for accessing configuration values.
 */
export interface ConfigAccessor<T> {
  /**
   * Get the current configuration.
   */
  get(): T;

  /**
   * Set a new configuration value.
   */
  set(value: T): void;

  /**
   * Reset to default configuration.
   */
  reset(): void;

  /**
   * Check if accessor has been initialized with a value.
   */
  isInitialized(): boolean;
}

/**
 * Create a configuration accessor instance.
 *
 * @param defaultValue - Default configuration value
 * @returns ConfigAccessor instance
 */
export function createConfigAccessor<T>(defaultValue: T): ConfigAccessor<T> {
  let current: T = defaultValue;
  let initialized = true;

  return {
    get: () => current,
    set: (value: T) => {
      current = value;
      initialized = true;
    },
    reset: () => {
      current = defaultValue;
    },
    isInitialized: () => initialized,
  };
}

/**
 * Create a lazy configuration accessor.
 *
 * The accessor starts uninitialized and must be explicitly set before use.
 *
 * @returns ConfigAccessor instance (uninitialized)
 */
export function createLazyConfigAccessor<T>(): ConfigAccessor<T | null> {
  let current: T | null = null;
  let initialized = false;

  return {
    get: () => {
      if (!initialized) {
        throw new Error("ConfigAccessor not initialized. Call set() first.");
      }
      return current;
    },
    set: (value: T) => {
      current = value;
      initialized = true;
    },
    reset: () => {
      current = null;
      initialized = false;
    },
    isInitialized: () => initialized,
  };
}

/**
 * Create a singleton configuration accessor.
 *
 * Provides global accessor pattern with initialization guard.
 * Useful for application-wide configuration access.
 *
 * @param defaultValue - Optional default value
 * @returns Object with accessor and management functions
 */
export function createSingletonAccessor<T>(defaultValue?: T) {
  let instance: ConfigAccessor<T | null> = createLazyConfigAccessor<T>();

  if (defaultValue !== undefined) {
    instance = createConfigAccessor<T>(defaultValue) as ConfigAccessor<T | null>;
  }

  return {
    get: () => instance.get(),
    set: (value: T) => instance.set(value),
    reset: () => instance.reset(),
    isInitialized: () => instance.isInitialized(),
    init: (value: T) => {
      if (instance.isInitialized() && defaultValue === undefined) {
        throw new Error("Singleton accessor already initialized.");
      }
      instance.set(value);
    },
  };
}

/**
 * Typed configuration key accessor.
 *
 * Provides type-safe access to specific config keys.
 */
export interface ConfigKeyAccessor<T, K extends keyof T> {
  /**
   * Get a specific configuration value by key.
   */
  get(): T[K];

  /**
   * Set a specific configuration value by key.
   */
  set(value: T[K]): void;
}

/**
 * Create a key-specific accessor from a parent accessor.
 *
 * @param parent - Parent configuration accessor
 * @param key - Key to access
 * @returns ConfigKeyAccessor for the specific key
 */
export function createKeyAccessor<T, K extends keyof T>(
  parent: ConfigAccessor<T>,
  key: K,
): ConfigKeyAccessor<T, K> {
  return {
    get: () => parent.get()[key],
    set: (value: T[K]) => {
      const current = parent.get();
      parent.set({ ...current, [key]: value });
    },
  };
}