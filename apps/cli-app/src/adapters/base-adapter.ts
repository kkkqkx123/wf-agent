/**
 * Base Adapter Class
 * Provides common adapter functionality
 */

import { getSDK } from "@wf-agent/sdk";
import { getOutput, type CLIOutput } from "../utils/output.js";
import { handleError as handleCLIError, type ErrorContext } from "../utils/error-handler.js";
import { CLIError } from "../types/cli-types.js";
import { isHeadlessMode } from "../utils/exit-manager.js";

/**
 * Base Adapter Class
 */
export class BaseAdapter {
  protected output: CLIOutput;
  protected sdk: ReturnType<typeof getSDK>;

  constructor() {
    this.output = getOutput();
    this.sdk = getSDK();
  }

  /**
   * Check if running in headless mode
   */
  protected isHeadlessMode(): boolean {
    return isHeadlessMode();
  }

  /**
   * Output result based on current mode
   */
  protected outputResult<T>(data: T, options?: { message?: string; success?: boolean }): void {
    this.output.result(data, options);
  }

  /**
   * Output error result based on current mode
   */
  protected outputError(error: Error | string, code?: string): void {
    this.output.errorResult(error, code);
  }

  /**
   * Output operation success message to both stdout and log
   * This ensures user visibility and test verification while maintaining audit trail
   */
  protected logOperation(message: string): void {
    // Output to stdout for user visibility and test verification
    this.output.success(message);
    // Also log for audit trail
    this.output.infoLog(message);
  }

  /**
   * Output operation failure message to both stderr and log
   */
  protected logOperationFailure(message: string): void {
    // Output to stderr for user visibility and test verification
    this.output.fail(message);
    // Also log for audit trail
    this.output.errorLog(message);
  }

  /**
   * 处理错误并转换为 CLIError
   */
  protected handleError(error: unknown, context: string): never {
    const cliError =
      error instanceof CLIError
        ? error
        : new CLIError(error instanceof Error ? error.message : String(error), "ADAPTER_ERROR");

    this.output.errorLog(`${context}: ${cliError.message}`);

    if (error instanceof Error && error.stack) {
      this.output.debugLog(error.stack);
    }

    throw cliError;
  }

  /**
   * 执行操作并处理错误
   */
  protected async executeWithErrorHandling<T>(
    operation: () => Promise<T>,
    context: string,
  ): Promise<T> {
    try {
      return await operation();
    } catch (error) {
      this.handleError(error, context);
    }
  }

  /**
   * 创建错误上下文
   */
  protected createErrorContext(operation: string, additional?: Record<string, any>): ErrorContext {
    return {
      operation,
      additionalInfo: additional,
    };
  }
}
