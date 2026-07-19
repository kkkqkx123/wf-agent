/**
 * Runtime Config Accessor
 * Base configuration accessor providing convenient getter methods.
 *
 * Provides a generic wrapper around configuration objects with
 * getter/setter/reset capabilities. Apps extend this with their
 * own domain-specific accessor methods.
 *
 * Usage:
 *   class MyConfigAccessor extends ConfigAccessor<MyConfig> {
 *     getDatabasePath(): string { return this.get().storage?.sqlite?.dbPath ?? "./default.db"; }
 *   }
 */

/**
 * Generic configuration accessor base class.
 * Wraps a configuration object with getter/setter/reset capabilities.
 */
export class ConfigAccessor<T extends object> {
  private config: T | null = null;
  private defaultConfig: T | null = null;

  constructor(config?: T) {
    if (config) {
      this.config = { ...config };
      this.defaultConfig = { ...config };
    }
  }

  /**
   * Get the underlying configuration object.
   */
  get(): T {
    if (!this.config) {
      throw new Error("Configuration not initialized. Call init() or set() first.");
    }
    return this.config;
  }

  /**
   * Set the full configuration object.
   */
  set(config: T): void {
    this.config = { ...config };
    if (!this.defaultConfig) {
      this.defaultConfig = { ...config };
    }
  }

  /**
   * Initialize with a configuration object.
   */
  init(config: T): void {
    this.config = { ...config };
    this.defaultConfig = { ...config };
  }

  /**
   * Reset to the default configuration that was set at initialization.
   */
  reset(): void {
    if (this.defaultConfig) {
      this.config = { ...this.defaultConfig };
    }
  }

  /**
   * Check if the accessor has been initialized.
   */
  isInitialized(): boolean {
    return this.config !== null;
  }
}