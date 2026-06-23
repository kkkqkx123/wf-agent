/**
 * Unified execution option types
 * All execution methods of the Core API accept this type of option
 */

/**
 * Execute the option
 */
export interface ExecutionOptions {
  /** Timeout period (in milliseconds), with a default value of 30000ms */
  timeout?: number;
  /** Number of retries, default is 0. */
  retries?: number;
  /** Retry delay (in milliseconds), default is 1000ms */
  retryDelay?: number;
  /** Whether to enable caching, with the default value being false. */
  cache?: boolean;
  /** Whether to enable logging, with the default value being true. */
  logging?: boolean;
  /** Whether to enable validation, with the default value being true. */
  validation?: boolean;
}

/**
 * Default execution option
 */
export const DEFAULT_EXECUTION_OPTIONS: Required<ExecutionOptions> = {
  timeout: 30000,
  retries: 0,
  retryDelay: 1000,
  cache: false,
  logging: true,
  validation: true,
};

/**
 * Merge execution options
 */
export function mergeExecutionOptions(
  options?: ExecutionOptions,
  defaults: Required<ExecutionOptions> = DEFAULT_EXECUTION_OPTIONS,
): Required<ExecutionOptions> {
  return {
    timeout: options?.timeout ?? defaults.timeout,
    retries: options?.retries ?? defaults.retries,
    retryDelay: options?.retryDelay ?? defaults.retryDelay,
    cache: options?.cache ?? defaults.cache,
    logging: options?.logging ?? defaults.logging,
    validation: options?.validation ?? defaults.validation,
  };
}
