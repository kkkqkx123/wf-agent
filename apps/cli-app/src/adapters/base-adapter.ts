/**
 * CLI Base Adapter Class
 * Provides common adapter functionality for CLI-specific adapters.
 *
 * Extends the shared BaseAppAdapter from @wf-agent/runtime with
 * CLI-specific output, error handling, and logging utilities.
 *
 * Inherits executeWithErrorHandling from BaseAppAdapter and overrides
 * handleOperationError to convert errors to CLIError.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import { BaseAppAdapter } from "@wf-agent/runtime/adapters";
import { getOutput, type CLIOutput } from "../utils/output.js";
import type { ErrorContext } from "../utils/error-handler.js";
import { CLIError } from "../types/cli-types.js";
import { isHeadless } from "../utils/mode-detector.js";
import { getSDKInstance } from "../index.js";

/**
 * CLI Base Adapter Class
 * Extends the runtime BaseAppAdapter with CLI-specific functionality.
 *
 * Maintains backward compatibility: adapters can call super() without
 * passing an SDK instance — it will be resolved from the global SDK.
 */
export class BaseAdapter extends BaseAppAdapter {
  protected output: CLIOutput;

  constructor(sdk?: SDKInstance) {
    // Resolve SDK: prefer explicit injection, fall back to global
    const resolvedSdk = sdk ?? getSDKInstance();
    if (!resolvedSdk) {
      throw new Error("SDK instance not initialized. Make sure the CLI app has started.");
    }
    super(resolvedSdk);
    this.output = getOutput();
  }

  /**
   * Check if running in headless mode
   */
  protected isHeadlessMode(): boolean {
    return isHeadless();
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
   * Handle error and convert to CLIError
   * Override of BaseAppAdapter.handleOperationError.
   * Converts unknown errors to CLIError, logs them, and throws.
   */
  protected override handleOperationError(error: unknown, context: string): never {
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
   * Create error context
   */
  protected createErrorContext(operation: string, additional?: Record<string, unknown>): ErrorContext {
    return {
      operation,
      additionalInfo: additional,
    };
  }
}
