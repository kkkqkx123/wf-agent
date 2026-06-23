/**
 * Environment Variable Mapping
 *
 * Provides centralized environment variable to configuration mapping.
 * This module standardizes how environment variables override config values.
 *
 * Design principles:
 * - Declarative mapping definition
 * - Type-safe parsing of env values
 * - Consistent naming conventions
 * - Pure functions (no side effects)
 */

/**
 * Environment variable parser function type.
 * Converts string environment variable value to target type.
 */
export type EnvParser<T = unknown> = (value: string) => T;

/**
 * Environment variable mapping entry.
 * Defines how an environment variable maps to a config field.
 */
export interface EnvMappingEntry<T = unknown> {
  /**
   * Environment variable name.
   */
  env: string;

  /**
   * Parser function to convert string to target type.
   */
  parser: EnvParser<T>;

  /**
   * Optional default value if env var is not set.
   */
  default?: T;

  /**
   * Whether the env var is required (throws if not set and no default).
   */
  required?: boolean;
}

/**
 * Environment variable mapping definition.
 * Maps config keys to their environment variable sources.
 */
export type EnvMapping<T> = {
  [K in keyof T]?: EnvMappingEntry<T[K]>;
};

/**
 * Standard environment variable parsers.
 */
export const EnvParsers = {
  /**
   * Parse string value (no conversion needed).
   */
  string: (value: string): string => value,

  /**
   * Parse integer value.
   */
  int: (value: string): number => parseInt(value, 10),

  /**
   * Parse float value.
   */
  float: (value: string): number => parseFloat(value),

  /**
   * Parse boolean value.
   * Accepts: "true", "1", "yes" → true; "false", "0", "no" → false
   */
  boolean: (value: string): boolean => {
    const lower = value.toLowerCase();
    return lower === "true" || lower === "1" || lower === "yes";
  },

  /**
   * Parse JSON value.
   */
  json: <T>(value: string): T => JSON.parse(value) as T,

  /**
   * Parse comma-separated list.
   */
  list: (value: string): string[] =>
    value.split(",").map((s) => s.trim()).filter((s) => s.length > 0),

  /**
   * Parse enum value with validation.
   */
  enum: <T extends string>(allowed: T[]) => (value: string): T => {
    if (!allowed.includes(value as T)) {
      throw new Error(
        `Invalid enum value: "${value}". Allowed values: ${allowed.join(", ")}`,
      );
    }
    return value as T;
  },
};

/**
 * Apply environment variable overrides to configuration.
 *
 * @param config - Base configuration object
 * @param mapping - Environment variable mapping definition
 * @returns Configuration with environment overrides applied
 */
export function applyEnvOverrides<T>(
  config: T,
  mapping: EnvMapping<T>,
): T {
  const result = { ...config } as T;

  for (const key in mapping) {
    if (Object.prototype.hasOwnProperty.call(mapping, key)) {
      const entry = mapping[key];
      if (!entry) continue;

      const envValue = process.env[entry.env];

      if (envValue !== undefined && envValue !== "") {
        try {
          (result as Record<string, unknown>)[key as string] = entry.parser(envValue);
        } catch (error) {
          throw new Error(
            `Failed to parse environment variable ${entry.env}: ${error instanceof Error ? error.message : String(error)}`,
            { cause: error },
          );
        }
      } else if (entry.required && entry.default === undefined) {
        throw new Error(`Required environment variable ${entry.env} is not set.`);
      } else if (entry.default !== undefined && !(key in (result as object))) {
        (result as Record<string, unknown>)[key as string] = entry.default;
      }
    }
  }

  return result;
}

/**
 * Create an environment variable mapping builder.
 *
 * Provides a fluent API for defining environment mappings.
 */
export class EnvMappingBuilder<T> {
  private mapping: EnvMapping<T> = {};

  /**
   * Add a string environment variable mapping.
   */
  string<K extends keyof T>(key: K, env: string, options?: { default?: string; required?: boolean }): this {
    this.mapping[key] = {
      env,
      parser: EnvParsers.string,
      ...options,
    } as EnvMappingEntry<T[K]>;
    return this;
  }

  /**
   * Add an integer environment variable mapping.
   */
  int<K extends keyof T>(key: K, env: string, options?: { default?: number; required?: boolean }): this {
    this.mapping[key] = {
      env,
      parser: EnvParsers.int,
      ...options,
    } as EnvMappingEntry<T[K]>;
    return this;
  }

  /**
   * Add a boolean environment variable mapping.
   */
  boolean<K extends keyof T>(key: K, env: string, options?: { default?: boolean; required?: boolean }): this {
    this.mapping[key] = {
      env,
      parser: EnvParsers.boolean,
      ...options,
    } as EnvMappingEntry<T[K]>;
    return this;
  }

  /**
   * Add a list environment variable mapping.
   */
  list<K extends keyof T>(key: K, env: string, options?: { default?: string[]; required?: boolean }): this {
    this.mapping[key] = {
      env,
      parser: EnvParsers.list,
      ...options,
    } as EnvMappingEntry<T[K]>;
    return this;
  }

  /**
   * Add a custom parser environment variable mapping.
   */
  custom<K extends keyof T>(key: K, env: string, parser: EnvParser<T[K]>, options?: { default?: T[K]; required?: boolean }): this {
    this.mapping[key] = {
      env,
      parser,
      ...options,
    };
    return this;
  }

  /**
   * Build the final environment mapping.
   */
  build(): EnvMapping<T> {
    return this.mapping;
  }
}

/**
 * Create a new environment mapping builder.
 */
export function createEnvMapping<T>(): EnvMappingBuilder<T> {
  return new EnvMappingBuilder<T>();
}

/**
 * Environment variable naming convention constants.
 *
 * Standard prefixes for different config scopes.
 */
export const EnvPrefixes = {
  CLI: "CLI_",
  SDK: "SDK_",
  GLOBAL: "GLOBAL_",
  WF: "WF_",
};

/**
 * Generate environment variable name from config key.
 *
 * @param prefix - Environment variable prefix
 * @param key - Configuration key
 * @returns Environment variable name
 */
export function toEnvName(prefix: string, key: string): string {
  return `${prefix}${key.toUpperCase().replace(/([a-z])([A-Z])/g, "$1_$2")}`;
}