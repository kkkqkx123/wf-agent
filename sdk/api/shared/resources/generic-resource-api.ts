/**
 * GenericResourceAPI - A generic resource API base class that provides a unified interface for CRUD operations and supports common features such as caching and error handling.
 *
 * Design Patterns Used:
 * - Template Method Pattern: Defines a general process, with the specific implementation provided by subclasses.
 * - Strategy Pattern: Supports different resource types through abstract methods.
 * - Interface Segregation: Split into Readable, Writable, and Clearable interfaces to avoid forcing subclasses to implement unused methods.
 *
 * Architecture:
 * - ReadableResourceAPI: Base class for read-only resources (get, getAll, has, count)
 * - WritableResourceAPI: Interface for write operations (create, update, delete)
 * - ClearableResourceAPI: Interface for clear operations
 * - CrudResourceAPI: Full CRUD implementation extending ReadableResourceAPI
 */

import type { ExecutionResult } from "../types/execution-result.js";
import { success, failure } from "../types/execution-result.js";
import { SDKError, ExecutionError as SDKExecutionError, ValidationError } from "@wf-agent/types";
import { isError, diffTimestamp, now } from "@wf-agent/common-utils";

// ============================================================================
// Interfaces - Define capabilities at the type level
// ============================================================================

/**
 * Interface for readable resources
 */
export interface ReadableResourceAPI<T, ID extends string | number, Filter = unknown> {
  /**
   * Get a single resource
   * @param id Resource ID
   * @returns Execution result containing the resource or null
   */
  get(id: ID): Promise<ExecutionResult<T | null>>;

  /**
   * Get all resources
   * @param filter Optional filter criteria
   * @returns Execution result containing array of resources
   */
  getAll(filter?: Filter): Promise<ExecutionResult<T[]>>;

  /**
   * Check if a resource exists
   * @param id Resource ID
   * @returns Execution result containing boolean
   */
  has(id: ID): Promise<ExecutionResult<boolean>>;

  /**
   * Get the count of resources
   * @returns Execution result containing count
   */
  count(): Promise<ExecutionResult<number>>;
}

/**
 * Interface for writable resources
 */
export interface WritableResourceAPI<T, ID extends string | number> {
  /**
   * Create a new resource
   * @param resource Resource object
   * @returns Execution result
   */
  create(resource: T): Promise<ExecutionResult<void>>;

  /**
   * Update a resource
   * @param id Resource ID
   * @param updates Partial updates
   * @returns Execution result
   */
  update(id: ID, updates: Partial<T>): Promise<ExecutionResult<void>>;

  /**
   * Delete a resource
   * @param id Resource ID
   * @returns Execution result
   */
  delete(id: ID): Promise<ExecutionResult<void>>;
}

/**
 * Interface for clearable resources
 */
export interface ClearableResourceAPI {
  /**
   * Clear all resources
   * @returns Execution result
   */
  clear(): Promise<ExecutionResult<void>>;
}

// ============================================================================
// Abstract Base Classes
// ============================================================================

/**
 * Base class for all resource APIs
 * Provides common error handling utilities
 */
export abstract class BaseResourceAPI {
  /**
   * Handle errors consistently
   * @param error Error object
   * @param operation Operation name
   * @param startTime Start time
   * @returns Execution result
   */
  protected handleError<T>(
    error: unknown,
    operation: string,
    startTime: number,
  ): ExecutionResult<T> {
    let sdkError: SDKError;

    // If it is already an SDKError (including all its subclasses), use it directly.
    if (error instanceof SDKError) {
      sdkError = error;
    }
    // If it's a regular error, convert it to SDKExecutionError.
    else if (isError(error)) {
      sdkError = new SDKExecutionError(
        error.message,
        undefined,
        undefined,
        {
          operation,
          originalError: error.name,
          stack: error.stack,
        },
        error,
      );
    }
    // For other types, convert to SDKExecutionError.
    else {
      sdkError = new SDKExecutionError(String(error), undefined, undefined, { operation });
    }

    // Return a failure result with detailed error information.
    return failure(sdkError, diffTimestamp(startTime, now()));
  }
}

/**
 * Read-only Resource API Base Class
 *
 * Provides read operations only. Subclasses only need to implement:
 * - getResource(id): Get a single resource
 * - getAllResources(): Get all resources
 *
 * @template T - Resource type
 * @template ID - Resource ID type (string or number)
 * @template Filter - Filter type
 */
export abstract class ReadonlyResourceAPI<T, ID extends string | number, Filter = unknown>
  extends BaseResourceAPI
  implements ReadableResourceAPI<T, ID, Filter>
{
  // ============================================================================
  // Abstract methods - Subclasses must implement these
  // ============================================================================

  /**
   * Get a single resource from the registry
   * @param id Resource ID
   * @returns Resource object; returns null if the resource does not exist
   */
  protected abstract getResource(id: ID): Promise<T | null>;

  /**
   * Get all resources from the registry
   * @returns Array of resources
   */
  protected abstract getAllResources(): Promise<T[]>;

  // ============================================================================
  // Protected methods - Subclasses can override for customization
  // ============================================================================

  /**
   * Apply filter criteria
   * @param resources Array of resources
   * @param filter Filter criteria
   * @returns Array of resources after filtering
   */
  protected applyFilter(resources: T[], filter: Filter): T[] {
    // Default implementation: return all resources without filtering
    // Subclasses can override this method to implement specific filtering logic
    return resources;
  }

  // ============================================================================
  // Public API Methods
  // ============================================================================

  /**
   * Get a single resource
   * @param id Resource ID
   * @returns Execution result
   */
  async get(id: ID): Promise<ExecutionResult<T | null>> {
    const startTime = now();

    try {
      const resource = await this.getResource(id);
      return success(resource, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "GET", startTime);
    }
  }

  /**
   * Get all resources
   * @param filter Optional filter criteria
   * @returns Execution result
   */
  async getAll(filter?: Filter): Promise<ExecutionResult<T[]>> {
    const startTime = now();

    try {
      let resources = await this.getAllResources();

      // Apply filter criteria
      if (filter) {
        resources = this.applyFilter(resources, filter);
      }

      return success(resources, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "GET_ALL", startTime);
    }
  }

  /**
   * Check if a resource exists
   * @param id Resource ID
   * @returns Execution result
   */
  async has(id: ID): Promise<ExecutionResult<boolean>> {
    const startTime = now();

    try {
      const resource = await this.getResource(id);
      return success(resource !== null, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "HAS", startTime);
    }
  }

  /**
   * Get the count of resources
   * @returns Execution result
   */
  async count(): Promise<ExecutionResult<number>> {
    const startTime = now();

    try {
      const resources = await this.getAllResources();
      return success(resources.length, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "COUNT", startTime);
    }
  }
}

/**
 * CRUD Resource API Base Class
 *
 * Provides full CRUD operations. Subclasses need to implement:
 * - getResource(id): Get a single resource
 * - getAllResources(): Get all resources
 * - createResource(resource): Create a resource
 * - updateResource(id, updates): Update a resource
 * - deleteResource(id): Delete a resource
 * - Optionally clearResources(): Clear all resources
 *
 * @template T - Resource type
 * @template ID - Resource ID type (string or number)
 * @template Filter - Filter type
 */
export abstract class CrudResourceAPI<T, ID extends string | number, Filter = unknown>
  extends ReadonlyResourceAPI<T, ID, Filter>
  implements WritableResourceAPI<T, ID>, ClearableResourceAPI
{
  // ============================================================================
  // Abstract methods - Subclasses must implement these
  // ============================================================================

  /**
   * Create a new resource
   * @param resource Resource object
   */
  protected abstract createResource(resource: T): Promise<void>;

  /**
   * Update a resource
   * @param id Resource ID
   * @param updates Update content
   */
  protected abstract updateResource(id: ID, updates: Partial<T>): Promise<void>;

  /**
   * Delete a resource
   * @param id Resource ID
   */
  protected abstract deleteResource(id: ID): Promise<void>;

  // ============================================================================
  // Protected methods - Subclasses can override for customization
  // ============================================================================

  /**
   * Validate resource (subclasses can override this method)
   * @param resource The resource object
   * @param context The validation context
   * @returns The validation result
   */
  protected async validateResource(
    _resource: T,
    _context?: unknown,
  ): Promise<{ valid: boolean; errors: string[] }> {
    // Default implementation: Subclasses can override this method.
    return { valid: true, errors: [] };
  }

  /**
   * Validate update content (subclasses can override this method)
   * @param updates The update content
   * @param context The validation context
   * @returns The validation result
   */
  protected async validateUpdate(
    _updates: Partial<T>,
    _context?: unknown,
  ): Promise<{ valid: boolean; errors: string[] }> {
    // Default implementation: Subclasses can override this method.
    return { valid: true, errors: [] };
  }

  /**
   * Clear resources (subclass implementation)
   * Default implementation throws error - subclasses must override if clear is supported
   */
  protected async clearResources(): Promise<void> {
    throw new Error("clearResources must be implemented by subclass");
  }

  // ============================================================================
  // Public API Methods
  // ============================================================================

  /**
   * Create a new resource
   * @param resource Resource object
   * @returns Execution result
   */
  async create(resource: T): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      // Validate resource
      const validation = await this.validateResource(resource);
      if (!validation.valid) {
        return failure(
          new ValidationError(
            `Validation failed: ${validation.errors.join(", ")}`,
            undefined,
            undefined,
            { errors: validation.errors },
          ),
          diffTimestamp(startTime, now()),
        );
      }

      await this.createResource(resource);
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "CREATE", startTime);
    }
  }

  /**
   * Update a resource
   * @param id Resource ID
   * @param updates Update content
   * @returns Execution result
   */
  async update(id: ID, updates: Partial<T>): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      // Validate update content
      const validation = await this.validateUpdate(updates);
      if (!validation.valid) {
        return failure(
          new ValidationError(
            `Validation failed: ${validation.errors.join(", ")}`,
            undefined,
            undefined,
            { errors: validation.errors },
          ),
          diffTimestamp(startTime, now()),
        );
      }

      await this.updateResource(id, updates);
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "UPDATE", startTime);
    }
  }

  /**
   * Delete a resource
   * @param id Resource ID
   * @returns Execution result
   */
  async delete(id: ID): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      await this.deleteResource(id);
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "DELETE", startTime);
    }
  }

  /**
   * Clear all resources
   * @returns Execution result
   */
  async clear(): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      await this.clearResources();
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "CLEAR", startTime);
    }
  }
}
