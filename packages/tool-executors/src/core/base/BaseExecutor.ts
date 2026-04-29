/**
 * Tool Executor Abstract Base Class
 * Provides general execution logic: parameter validation, retry mechanism, timeout control, and standardized result formatting
 */

import type { Tool, ToolExecutionOptions, ToolExecutionResult } from "@wf-agent/types";
import { IToolExecutor } from "../interfaces/IToolExecutor.js";
import { ParameterValidator } from "./ParameterValidator.js";
import { RetryStrategy } from "./RetryStrategy.js";
import { TimeoutController } from "./TimeoutController.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";

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
   * @returns: Standardized execution results
   */
  async execute(
    tool: Tool,
    parameters: Record<string, unknown>,
    options: ToolExecutionOptions = {},
    executionId?: string,
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
          () => this.doExecute(tool, parameters, executionId),
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
   * @returns Execution result
   */
  protected abstract doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    executionId?: string,
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
