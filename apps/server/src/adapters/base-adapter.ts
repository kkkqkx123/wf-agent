/**
 * Server Base Adapter
 *
 * Abstract base class for all Server adapters.
 * Provides common functionality for resource access patterns.
 *
 * Extends the shared BaseAppAdapter from @wf-agent/runtime with
 * server-specific pagination, error handling, and logging.
 *
 * Inherits executeWithErrorHandling from BaseAppAdapter and overrides
 * handleOperationError to convert errors to ApiError.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import { BaseAppAdapter, AdapterError } from "@wf-agent/runtime/adapters";
import { getOutput } from "../utils/output.js";

export interface QueryOptions {
  offset?: number;
  limit?: number;
  sort?: string;
  filter?: Record<string, any>;
}

export interface PaginatedResponse<T> {
  data: T[];
  meta?: {
    total?: number;
    offset?: number;
    limit?: number;
  };
}

export class ApiError extends AdapterError {
  constructor(
    code: string,
    message: string,
    public details?: Record<string, any>
  ) {
    super(code, message);
    this.name = "ApiError";
  }
}

/**
 * Base adapter class for all server resource adapters
 */
export abstract class BaseAdapter extends BaseAppAdapter {
  protected logger = getOutput();
  protected defaultPageSize = 20;

  constructor(sdk: SDKInstance) {
    super(sdk);
  }

  /**
   * Get the resource name (for logging)
   */
  abstract getResourceName(): string;

  /**
   * Validate adapter initialization
   */
  async validate(): Promise<void> {
    if (!this.sdk) {
      throw new ApiError(
        "ADAPTER_NOT_INITIALIZED",
        "SDK not initialized for adapter"
      );
    }
  }

  /**
   * Handle adapter errors consistently.
   * Override of BaseAppAdapter.handleOperationError.
   * Converts unknown errors to ApiError, logs them, and throws.
   */
  protected override handleOperationError(error: unknown, context: string): never {
    const message = error instanceof Error ? error.message : String(error);
    this.logger.errorLog(
      `${this.getResourceName()} adapter error in ${context}: ${message}`
    );

    if (error instanceof ApiError) {
      throw error;
    }

    throw new ApiError(
      "ADAPTER_ERROR",
      `Failed to ${context} on ${this.getResourceName()}`,
      {
        originalError: message,
        operation: context,
        resource: this.getResourceName(),
      }
    );
  }

  /**
   * Apply pagination to items
   */
  protected applyPagination<T>(
    items: T[],
    query?: QueryOptions
  ): PaginatedResponse<T> {
    const offset = query?.offset || 0;
    const limit = query?.limit || this.defaultPageSize;

    if (offset < 0 || limit < 1) {
      throw new ApiError(
        "INVALID_PAGINATION",
        "Invalid offset or limit values",
        { offset, limit }
      );
    }

    return {
      data: items.slice(offset, offset + limit),
      meta: {
        total: items.length,
        offset,
        limit,
      },
    };
  }

  /**
   * Log adapter operation
   */
  protected logOperation(operation: string, params?: Record<string, any>) {
    const paramStr =
      params && Object.keys(params).length > 0
        ? ` with ${JSON.stringify(params)}`
        : "";
    this.logger.debugLog(`${this.getResourceName()}.${operation}${paramStr}`);
  }
}
