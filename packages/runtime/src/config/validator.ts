/**
 * Runtime Config Validator
 * Base configuration validation utilities.
 *
 * Provides a base validator that apps can extend with their own
 * validation rules. Handles common validation patterns like
 * storage configuration validation.
 *
 * Usage:
 *   class MyValidator extends ConfigValidator<MyConfig> {
 *     protected validateApp(): string[] {
 *       return [...this.validateStorage(), ...this.validateOutput()];
 *     }
 *   }
 */

/**
 * Generic configuration validator base class.
 * Apps extend this and override validateApp() to add app-specific checks.
 */
export class ConfigValidator<T extends { storage?: { type?: string; sqlite?: unknown }; output?: Record<string, unknown> }> {
  /**
   * Validate a configuration object.
   * Override validateApp() to add app-specific validation.
   *
   * @param config Configuration object to validate
   * @returns Validation result with errors if any
   */
  validate(config: T): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    // Common storage validation
    errors.push(...this.validateStorage(config));

    // App-specific validation (override in subclass)
    errors.push(...this.validateApp(config));

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Validate configuration and throw if invalid.
   */
  validateOrThrow(config: T): void {
    const { valid, errors } = this.validate(config);
    if (!valid) {
      throw new Error(`Configuration validation failed:\n${errors.join("\n")}`);
    }
  }

  /**
   * Common storage configuration validation.
   */
  protected validateStorage(config: T): string[] {
    const errors: string[] = [];

    if (config.storage) {
      if (config.storage.type === "sqlite" && !config.storage.sqlite) {
        errors.push("SQLite storage configuration is required when storage type is 'sqlite'");
      }
    }

    return errors;
  }

  /**
   * App-specific validation — override in subclasses.
   */
  protected validateApp(_config: T): string[] {
    return [];
  }
}

/**
 * Singleton-based validator helpers for simple use cases.
 * For apps that don't need subclassing, use these functions directly.
 */

let globalValidator: ConfigValidator<Record<string, unknown>> | null = null;

/**
 * Get or create the global validator instance.
 */
function getValidator(): ConfigValidator<Record<string, unknown>> {
  if (!globalValidator) {
    globalValidator = new ConfigValidator();
  }
  return globalValidator;
}

/**
 * Quick validation using the global validator.
 */
export function validateConfig<T extends { storage?: { type?: string; sqlite?: unknown }; output?: Record<string, unknown> }>(
  config: T,
): { valid: boolean; errors: string[] } {
  return getValidator().validate(config as unknown as Record<string, unknown>);
}

/**
 * Quick validation with throw using the global validator.
 */
export function validateConfigOrThrow<T extends { storage?: { type?: string; sqlite?: unknown }; output?: Record<string, unknown> }>(
  config: T,
): void {
  getValidator().validateOrThrow(config as unknown as Record<string, unknown>);
}