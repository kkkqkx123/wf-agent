/**
 * Core base classes for tool execution
 * Provides parameter validation, retry mechanism, timeout control, and standardized result formatting
 */

import { z } from "zod";
import type { Tool, ToolExecutionOptions, ToolExecutionResult } from "@wf-agent/types";
import { RuntimeValidationError, TimeoutError, HttpError, NetworkError } from "@wf-agent/types";
import {
  InternalServerError,
  RateLimitError,
  ServiceUnavailableError,
  executeWithRetry,
  type RetryConfig,
} from "../../transport/http/index.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import { combineTimeoutWithSignal } from "../../../shared/utils/timeout/index.js";
import type { IToolExecutor } from "./interfaces.js";

// ============================================================================
// Parameter Validator
// ============================================================================

/**
 * Parameter Validator
 * Responsible for verifying whether tool parameters conform to the defined schema
 */
export class ParameterValidator {
  /**
   * Verify tool parameters
   * @param tool: Tool definition
   * @param parameters: Tool parameters
   * @throws ValidationError: If parameter validation fails
   */
  validate(tool: Tool, parameters: Record<string, unknown>): void {
    const schema = this.buildSchema(tool);
    const result = schema.safeParse(parameters);

    if (!result.success) {
      const firstError = result.error.issues[0];
      if (!firstError) {
        throw new RuntimeValidationError("Parameter validation failed", {
          operation: "validate",
          field: "parameters",
          value: parameters,
        });
      }
      const field = firstError.path.join(".");
      throw new RuntimeValidationError(firstError.message, {
        operation: "validate",
        field: field,
        value: parameters,
      });
    }
  }

  /**
   * Construct a parameter validation schema
   * @param tool: Tool definition
   * @returns: Zod schema
   */
  private buildSchema(tool: Tool): z.ZodType<Record<string, unknown>> {
    const shape: Record<string, z.ZodTypeAny> = {};

    for (const [paramName, paramSchema] of Object.entries(tool.parameters.properties)) {
      let zodSchema = this.buildTypeSchema(paramSchema.type);

      // Add enumeration validation
      if (paramSchema.enum && paramSchema.enum.length > 0) {
        zodSchema = zodSchema.pipe(z.enum(paramSchema.enum as [string, ...string[]]));
      }

      // Add format validation
      if (paramSchema.format && typeof paramSchema.format === "string") {
        zodSchema = zodSchema.pipe(this.buildFormatSchema(paramSchema.format));
      }

      // Set whether it is mandatory
      if (tool.parameters.required.includes(paramName)) {
        shape[paramName] = zodSchema;
      } else {
        shape[paramName] = zodSchema.optional();
      }
    }

    return z.object(shape);
  }

  /**
   * Construct a type schema
   * @param type: A string representing the type
   * @returns: A Zod schema
   */
  private buildTypeSchema(type: string): z.ZodTypeAny {
    switch (type) {
      case "string":
        return z.string();
      case "number":
        return z.number();
      case "boolean":
        return z.boolean();
      case "array":
        return z.array(z.unknown());
      case "object":
        return z.record(z.string(), z.unknown());
      default:
        return z.unknown();
    }
  }

  /**
   * Build a format schema
   * @param format: The format string
   * @returns: A Zod schema
   */
  private buildFormatSchema(format: string): z.ZodTypeAny {
    switch (format) {
      case "uri":
        return z.string().url();
      case "email":
        return z.string().email();
      case "uuid":
        return z.string().uuid();
      case "date-time":
        return z.string().datetime();
      default:
        return z.unknown();
    }
  }
}

// ============================================================================
// Retry Strategy
// ============================================================================

/**
 * Retry strategy configuration
 */
export interface RetryStrategyConfig {
  /** Maximum number of retries */
  maxRetries: number;
  /** Basic latency time (in milliseconds) */
  baseDelay: number;
  /** Should exponential backoff be used? */
  exponentialBackoff: boolean;
  /** Maximum delay time (in milliseconds) */
  maxDelay?: number;
}

/**
 * Retry Strategy
 * Wraps the `executeWithRetry` function from `common-utils` to provide a more user-friendly interface.
 */
export class RetryStrategy {
  private config: RetryStrategyConfig;

  constructor(config: RetryStrategyConfig) {
    this.config = config;
  }

  /**
   * Determine whether a retry should be attempted
   * @param error The error object
   * @param retryCount The current number of retries
   * @returns Whether a retry should be performed
   */
  shouldRetry(error: Error, retryCount: number): boolean {
    // Exceeded the maximum number of retries.
    if (retryCount >= this.config.maxRetries) {
      return false;
    }

    // TimeoutError - Retry after timeout
    if (error instanceof TimeoutError) {
      return true;
    }

    // Specific HTTP error types - Check these first
    if (error instanceof RateLimitError) {
      return true;
    }

    if (error instanceof InternalServerError) {
      return true;
    }

    if (error instanceof ServiceUnavailableError) {
      return true;
    }

    // Generic HttpError - Determine based on the status code (as a fallback mechanism)
    if (error instanceof HttpError) {
      // 5xx Server Error Retry
      if (error.statusCode >= 500 && error.statusCode < 600) {
        return true;
      }

      // Other HTTP errors are not retried (4xx client errors).
      return false;
    }

    // NetworkError - Retry due to a network connection error.
    if (error instanceof NetworkError) {
      return true;
    }

    return false;
  }

  /**
   * Get the retry delay time
   * @param retryCount: The current number of retries
   * @returns: Delay time in milliseconds
   */
  getRetryDelay(retryCount: number): number {
    let delay: number;

    if (this.config.exponentialBackoff) {
      // Exponential backoff: baseDelay * 2^retryCount
      delay = this.config.baseDelay * Math.pow(2, retryCount);
    } else {
      // Fixed delay
      delay = this.config.baseDelay;
    }

    // Apply the maximum latency limit.
    if (this.config.maxDelay && delay > this.config.maxDelay) {
      delay = this.config.maxDelay;
    }

    return delay;
  }

  /**
   * Execute a function with retries
   * @param fn The asynchronous function to be executed
   * @returns The return value of the function
   */
  async execute<T>(fn: () => Promise<T>): Promise<T> {
    const retryConfig: RetryConfig = {
      maxRetries: this.config.maxRetries,
      baseDelay: this.config.baseDelay,
      maxDelay: this.config.maxDelay,
    };

    return executeWithRetry(fn, retryConfig);
  }

  /**
   * Create a default retry policy
   */
  static createDefault(): RetryStrategy {
    return new RetryStrategy({
      maxRetries: 3,
      baseDelay: 1000,
      exponentialBackoff: true,
      maxDelay: 30000,
    });
  }

  /**
   * Create a no-retry strategy
   */
  static createNoRetry(): RetryStrategy {
    return new RetryStrategy({
      maxRetries: 0,
      baseDelay: 0,
      exponentialBackoff: false,
    });
  }

  /**
   * Create a custom retry strategy
   */
  static createCustom(config: Partial<RetryStrategyConfig>): RetryStrategy {
    return new RetryStrategy({
      maxRetries: config.maxRetries ?? 3,
      baseDelay: config.baseDelay ?? 1000,
      exponentialBackoff: config.exponentialBackoff ?? true,
      maxDelay: config.maxDelay ?? 30000,
    });
  }
}

// ============================================================================
// Timeout Controller
// ============================================================================

/**
 * Timeout Controller
 *
 * Lightweight wrapper for tool execution timeout control.
 * Internally reuses core timeout utility functions to avoid code duplication.
 */
export class TimeoutController {
  constructor(private defaultTimeout: number = 30000) {}

  /**
   * Timed execution
   * @param fn The function to be executed
   * @param timeout The timeout period in milliseconds, using the default value set by the constructor by default
   * @param signal Optional abort signal for external cancellation
   * @returns The execution result
   * @throws TimeoutError If the timeout is reached
   */
  async executeWithTimeout<T>(
    fn: () => Promise<T>,
    timeout?: number,
    signal?: AbortSignal,
  ): Promise<T> {
    const actualTimeout = timeout ?? this.defaultTimeout;

    // Handle zero or negative timeout (no timeout)
    if (actualTimeout <= 0) {
      return await fn();
    }

    // Combine timeout with abort signal if provided
    const { clearTimeout } = combineTimeoutWithSignal(actualTimeout, signal);

    try {
      // Execute function and race against timeout
      const result = await Promise.race([
        fn(),
        new Promise<never>((_, reject) => {
          setTimeout(() => {
            reject(
              new TimeoutError(`Tool execution timeout after ${actualTimeout}ms`, actualTimeout),
            );
          }, actualTimeout);
        }),
      ]);

      return result;
    } catch (error) {
      // If aborted via signal, throw appropriate error
      if (signal?.aborted) {
        throw new Error("Tool execution aborted", { cause: error });
      }
      throw error;
    } finally {
      // Clean up resources
      clearTimeout();
    }
  }

  /**
   * Create a default timeout controller
   */
  static createDefault(): TimeoutController {
    return new TimeoutController(30000);
  }

  /**
   * Create a timeout-free controller
   */
  static createNoTimeout(): TimeoutController {
    return new TimeoutController(0);
  }
}

// ============================================================================
// Base Executor
// ============================================================================

/**
 * Tool Executor Abstract Base Class
 * All concrete executors should inherit from this class.
 */
export abstract class BaseExecutor implements IToolExecutor {
  protected validator: ParameterValidator;
  protected retryStrategy: RetryStrategy;
  protected timeoutController: TimeoutController;

  constructor(
    validator?: ParameterValidator,
    retryStrategy?: RetryStrategy,
    timeoutController?: TimeoutController,
  ) {
    this.validator = validator ?? new ParameterValidator();
    this.retryStrategy = retryStrategy ?? RetryStrategy.createDefault();
    this.timeoutController = timeoutController ?? TimeoutController.createDefault();
  }

  /**
   * Execution tool (with validation, retry, and timeout)
   * @param tool: Tool definition
   * @param parameters: Tool parameters
   * @param options: Execution options
   * @param executionId: Execution ID (optional, used for execution isolation in stateful tools)
   * @param context: Execution context (optional, for interactive tools)
   * @returns: Standardized execution results
   */
  async execute(
    tool: Tool,
    parameters: Record<string, unknown>,
    options: ToolExecutionOptions = {},
    executionId?: string,
    context?: Record<string, unknown>,
  ): Promise<ToolExecutionResult> {
    const startTime = now();
    const { timeout = 30000, retries = 3, retryDelay = 1000, exponentialBackoff = true } = options;

    // Verify parameters
    this.validateParameters(tool, parameters);

    // Create a temporary retry strategy (using the configuration from the options)
    const tempRetryStrategy = new RetryStrategy({
      maxRetries: retries,
      baseDelay: retryDelay,
      exponentialBackoff,
    });

    // Execution tool (with retry)
    let lastError: Error | undefined;
    let retryCount = 0;

    for (let i = 0; i <= retries; i++) {
      try {
        // Execution tool (with timeout and abort signals)
        const result = await this.timeoutController.executeWithTimeout(
          () => this.doExecute(tool, parameters, executionId, context),
          timeout,
          options.signal,
        );

        const executionTime = diffTimestamp(startTime, now());

        return {
          success: true,
          result,
          executionTime,
          retryCount,
        };
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        retryCount = i;

        // Check whether a retry should be attempted.
        if (i < retries && tempRetryStrategy.shouldRetry(lastError, i)) {
          // Calculate the retry delay
          const delay = tempRetryStrategy.getRetryDelay(i);

          // Waiting for retry delay
          await this.sleep(delay);
          continue;
        }

        // If no retry is performed or the number of retries is exhausted, an error is thrown.
        break;
      }
    }

    // Execution failed.
    const executionTime = diffTimestamp(startTime, now());
    const errorMessage = lastError?.message || "Unknown error";

    return {
      success: false,
      error: errorMessage,
      executionTime,
      retryCount,
    };
  }

  /**
   * Verify tool parameters
   * @param tool: Tool definition
   * @param parameters: Tool parameters
   * @throws ValidationError: If parameter validation fails
   */
  validateParameters(tool: Tool, parameters: Record<string, unknown>): void {
    this.validator.validate(tool, parameters);
  }

  /**
   * Specific implementation of the execution tool (implemented by a subclass)
   * @param tool Tool definition
   * @param parameters Tool parameters
   * @param executionId Execution ID (optional, used for execution isolation in stateful tools)
   * @param context Execution context (optional, for interactive tools)
   * @returns Execution result
   */
  protected abstract doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    executionId?: string,
    context?: Record<string, unknown>,
  ): Promise<unknown>;

  /**
   * Specify sleep time
   * @param ms Sleep duration in milliseconds
   */
  protected async sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Get the executor type (implemented by the subclass)
   */
  abstract getExecutorType(): string;
}
