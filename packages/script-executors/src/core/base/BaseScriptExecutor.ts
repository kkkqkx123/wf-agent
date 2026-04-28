/**
 * Script Executor Abstract Base Class
 * Provides general execution logic: retry, timeout, standardization of result format
 * Note: Script validation is completed by the SDK during configuration loading; the executor does not re-perform the validation.
 */

import type { Script, ScriptExecutionOptions, ScriptExecutionResult } from "@wf-agent/types";
import { IScriptExecutor } from "../interfaces/IScriptExecutor.js";
import { RetryStrategy } from "./RetryStrategy.js";
import { TimeoutController } from "./TimeoutController.js";
import type { ExecutionContext, ExecutionOutput, ExecutorConfig } from "../types.js";
import { now, diffTimestamp } from "@wf-agent/common-utils";
import { createModuleLogger } from "../../logger.js";

const logger = createModuleLogger("base-executor");

/**
 * Script Executor Abstract Base Class
 * All concrete executors should inherit from this class
 */
export abstract class BaseScriptExecutor implements IScriptExecutor {
  protected retryStrategy: RetryStrategy;
  protected timeoutController: TimeoutController;
  protected config: ExecutorConfig;

  constructor(config?: ExecutorConfig) {
    this.config = config ?? { type: "SHELL" };
    this.retryStrategy = new RetryStrategy({
      maxRetries: config?.maxRetries,
      baseDelay: config?.retryDelay,
      exponentialBackoff: config?.exponentialBackoff,
    });
    this.timeoutController = new TimeoutController({
      defaultTimeout: config?.timeout,
    });
  }

  /**
   * Execute the script (including validation, retries, and timeouts)
   * @param script: Script definition
   * @param options: Execution options
   * @param context: Execution context
   * @returns: Standardized execution result
   */
  async execute(
    script: Script,
    options: ScriptExecutionOptions = {},
    context?: ExecutionContext,
  ): Promise<ScriptExecutionResult> {
    const startTime = now();
    const {
      timeout = this.config.timeout ?? 30000,
      retries = this.config.maxRetries ?? 3,
      retryDelay = this.config.retryDelay ?? 1000,
      exponentialBackoff = this.config.exponentialBackoff ?? true,
    } = options;

    logger.debug("Starting script execution", {
      scriptName: script.name,
      scriptType: script.type,
      timeout,
      retries,
    });

    // Create a temporary retry strategy (using the configuration from the options)
    const tempRetryStrategy = new RetryStrategy({
      maxRetries: retries,
      baseDelay: retryDelay,
      exponentialBackoff,
    });

    // Execute the script (with retries).
    let lastError: Error | undefined;
    let retryCount = 0;

    for (let i = 0; i <= retries; i++) {
      try {
        // Execute the script (with timeout and stop signals)
        const output = await this.timeoutController.executeWithTimeout(
          () => this.doExecute(script, context),
          timeout,
          options.signal,
        );

        const executionTime = diffTimestamp(startTime, now());

        logger.info("Script execution completed", {
          scriptName: script.name,
          scriptType: script.type,
          exitCode: output.exitCode,
          executionTime,
          retryCount: i,
        });

        return {
          success: output.exitCode === 0,
          scriptName: script.name,
          scriptType: script.type,
          stdout: output.stdout,
          stderr: output.stderr,
          exitCode: output.exitCode,
          executionTime,
          error: output.exitCode !== 0 ? output.stderr : undefined,
        };
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        retryCount = i;

        logger.warn("Script execution failed, checking retry", {
          scriptName: script.name,
          scriptType: script.type,
          attempt: i + 1,
          maxRetries: retries,
          error: lastError.message,
        });

        // Check whether a retry should be attempted.
        if (i < retries && tempRetryStrategy.shouldRetry(lastError, i)) {
          // Calculate the retry delay
          const delay = tempRetryStrategy.getRetryDelay(i);

          logger.debug("Retrying script execution", {
            scriptName: script.name,
            delay,
            nextAttempt: i + 2,
          });

          // Wait for retry delay
          await this.sleep(delay);
          continue;
        }

        // If no retry is attempted or the number of retries is exhausted, an error is thrown.
        break;
      }
    }

    // Execution failed.
    const executionTime = diffTimestamp(startTime, now());
    const errorMessage = lastError?.message || "Unknown error";

    logger.error("Script execution failed after all retries", {
      scriptName: script.name,
      scriptType: script.type,
      error: errorMessage,
      executionTime,
      retryCount,
    });

    return {
      success: false,
      scriptName: script.name,
      scriptType: script.type,
      error: errorMessage,
      executionTime,
      retryCount,
    };
  }

  /**
   * Verify script configuration
   * Note: This method is deprecated; script verification is now done by the SDK during configuration loading.
   * @deprecated Script verification is performed by the SDK when the configuration is loaded, and the executor no longer performs duplicate verifications.
   * @param _script The script definition
   * @returns The verification result (always returns "success")
   */
  validate(_script: Script): { valid: boolean; errors: string[] } {
    // Script validation is performed by the SDK during configuration loading, so the executor does not need to perform the validation again.
    // This return is successful to maintain interface compatibility.
    return { valid: true, errors: [] };
  }

  /**
   * The specific implementation of executing the script (implemented by a subclass)
   * @param script The script definition
   * @param context The execution context
   * @returns The execution output
   */
  protected abstract doExecute(
    script: Script,
    context?: ExecutionContext,
  ): Promise<ExecutionOutput>;

  /**
   * Specify sleep time
   * @param ms Sleep duration in milliseconds
   */
  protected async sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Clean up resources
   */
  async cleanup(): Promise<void> {
    // Subclasses can override this method to implement specific cleaning logic.
  }

  /**
   * Obtain the executor type (implemented by subclasses)
   */
  abstract getExecutorType(): string;

  /**
   * Get the supported script types (implemented by subclasses)
   * @returns Array of supported script types
   */
  abstract getSupportedTypes(): import("@wf-agent/types").ScriptType[];
}
