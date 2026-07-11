/**
 * Base Adapter
 *
 * Abstract base class for all Server adapters.
 * Provides common functionality for resource access patterns.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
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

export class ApiError extends Error {
  constructor(
    public code: string,
    message: string,
    public details?: Record<string, any>
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/**
 * Base adapter class for all resource adapters
 */
export abstract class BaseAdapter {
  protected logger = getOutput();
  protected defaultPageSize = 20;

  constructor(protected sdk: SDKInstance) {}

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
   * Handle adapter errors consistently
   */
  protected async handleError(
    error: unknown,
    operation: string
  ): Promise<never> {
    const message = error instanceof Error ? error.message : String(error);
    this.logger.errorLog(
      `${this.getResourceName()} adapter error in ${operation}: ${message}`
    );

    if (error instanceof ApiError) {
      throw error;
    }

    throw new ApiError(
      "ADAPTER_ERROR",
      `Failed to ${operation} on ${this.getResourceName()}`,
      {
        originalError: message,
        operation,
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
