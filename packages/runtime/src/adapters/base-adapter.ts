/**
 * Runtime BaseAdapter
 * Shared base class for application adapters.
 *
 * Provides common SDK instance access and error handling.
 * CLI and Server apps extend this with their own output/transport semantics.
 *
 * This is intentionally minimal:
 * - cli-app adapters add CLI-specific formatting and user interaction
 * - server adapters add HTTP-friendly patterns (pagination, API responses)
 */

import type { SDKInstance } from "@wf-agent/sdk/api";

/**
 * Base adapter error
 */
export class AdapterError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public override readonly cause?: unknown,
  ) {
    super(message);
    this.name = "AdapterError";
  }
}

/**
 * Shared base adapter class
 * Provides SDK instance access and common error handling.
 */
export abstract class BaseAppAdapter {
  constructor(protected sdk: SDKInstance) {}

  /**
   * Get the underlying SDK instance
   */
  getSDK(): SDKInstance {
    return this.sdk;
  }

  /**
   * Execute an async operation with error handling.
   *
   * Wraps the operation in try/catch and delegates error handling
   * to the subclass via handleOperationError(). Subclasses override
   * this method to provide app-specific error formatting (CLIError
   * for cli-app, ApiError for server).
   *
   * Usage:
   *   return this.executeWithErrorHandling(async () => {
   *     const result = await this.sdk.xxx.get(id);
   *     return result;
   *   }, "get xxx");
   *
   * @param operation Async operation to execute
   * @param context Human-readable operation description (for error messages)
   * @returns Result of the operation
   */
  protected async executeWithErrorHandling<T>(
    operation: () => Promise<T>,
    context: string,
  ): Promise<T> {
    try {
      return await operation();
    } catch (error) {
      this.handleOperationError(error, context);
    }
  }

  /**
   * Handle an operation error.
   * Override in subclasses to provide app-specific error handling.
   * By default, re-throws the error as-is.
   */
  protected handleOperationError(error: unknown, _context: string): never {
    throw error;
  }
}