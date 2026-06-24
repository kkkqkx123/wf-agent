/**
 * GenericResourceAPI - A generic resource API base class that provides a unified interface for CRUD operations and supports common features such as caching and error handling.
 *
 * Design Patterns Used:
 * - Template Method Pattern: Defines a general process, with the specific implementation provided by subclasses.
 * - Strategy Pattern: Supports different resource types through abstract methods.
 * - Interface Segregation: Split into Readable, Writable, and Clearable interfaces to avoid forcing subclasses to implement unused methods.
 * - Semantic Separation: Query operations return raw data (simpler API), Command operations return ExecutionResult (for complex operations)
 *
 * Architecture:
 * - QueryableResourceAPI: Read operations with simplified API (no ExecutionResult wrapping)
 * - WritableResourceAPI: Write operations returning ExecutionResult
 * - ClearableResourceAPI: Clear operations
 * - SimplifiedCrudResourceAPI: Combines Query + Write operations (RECOMMENDED)
 */

import type { ExecutionResult } from "../types/execution-result.js";
import { success, failure } from "../types/execution-result.js";
import {
  SDKError,
  ExecutionError as SDKExecutionError,
  ValidationError,
  ConfigurationValidationError,
} from "@wf-agent/types";
import { isError, diffTimestamp, now } from "@wf-agent/common-utils";
import type { DeleteCheckResult } from "../../../shared/registry/types.js";

/**
 * Migration Guide: Query/Command Semantic Separation
 *
 * Overview:
 * SDK API now separates read operations (Query) and write operations (Command):
 * - Query operations: return raw data (T | null, T[], boolean, number)
 * - Command operations: return ExecutionResult for comprehensive error handling
 *
 * Recommended Base Classes:
 * 1. QueryableResourceAPI<T, ID, Filter>
 *    - Simplified read operations
 *    - No ExecutionResult wrapping
 *    - Direct throws on error
 *    - Use for simple, low-error-rate operations
 *
 * 2. SimplifiedCrudResourceAPI<T, ID, Filter> (RECOMMENDED)
 *    - Combines QueryableResourceAPI (reads) + WritableResourceAPI (writes)
 *    - Best of both worlds
 *    - Use this for new implementations
 *
 * Migration Path for Existing Implementations:
 *
 * Step 1: If only using read operations
 *   OLD: class MyAPI extends ReadonlyResourceAPI<T, ID, Filter> { }
 *   NEW: class MyAPI extends QueryableResourceAPI<T, ID, Filter> { }
 *   - Update callers to handle raw types instead of ExecutionResult
 *
 * Step 2: If using both read and write operations
 *   OLD: class MyAPI extends CrudResourceAPI<T, ID, Filter> { }
 *   NEW: class MyAPI extends SimplifiedCrudResourceAPI<T, ID, Filter> { }
 *   - Reads become: const item = await api.get(id);  // returns T | null
 *   - Writes stay: const result = await api.create(item);  // returns ExecutionResult
 *
 * Step 3: Update caller code
 *   OLD:
 *     const result = await api.get(id);
 *     if (isSuccess(result)) {
 *       const item = result.data;
 *     }
 *
 *   NEW (QueryableResourceAPI):
 *     const item = await api.get(id);  // throws on error or returns T | null
 *
 *   NEW (Command operations):
 *     const result = await api.create(item);  // returns ExecutionResult
 *     if (isSuccess(result)) { // success }
 */

// ============================================================================
// Simplified Query Interfaces - Semantic separation of read operations
// ============================================================================

/**
 * Simplified Query Interface for Resources
 * Returns raw data without ExecutionResult wrapping
 *
 * Use this for simple read operations that rarely fail
 *
 * @template T - Resource type
 * @template ID - Resource ID type (string or number)
 * @template Filter - Filter type
 */
export interface QueryableResourceAPI<T, ID extends string | number, Filter = unknown> {
  /**
   * Get a single resource
   * @param id Resource ID
   * @returns Resource object or null if not found
   * @throws Error if operation fails
   */
  get(id: ID): Promise<T | null>;

  /**
   * Get all resources
   * @param filter Optional filter criteria
   * @returns Array of resources
   * @throws Error if operation fails
   */
  getAll(filter?: Filter): Promise<T[]>;

  /**
   * Check if a resource exists
   * @param id Resource ID
   * @returns Boolean indicating existence
   * @throws Error if operation fails
   */
  has(id: ID): Promise<boolean>;

  /**
   * Get the count of resources
   * @returns Resource count
   * @throws Error if operation fails
   */
  count(): Promise<number>;
}

// ============================================================================
// Writable and Clearable Interfaces
// ============================================================================

/**
 * Interface for writable resources
 * All write operations return ExecutionResult for comprehensive error handling
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
 * Simplified Query API Base Class
 * Provides read operations with simplified return types (no ExecutionResult wrapping)
 *
 * Subclasses need to implement:
 * - getResource(id): Get a single resource
 * - getAllResources(): Get all resources
 *
 * @template T - Resource type
 * @template ID - Resource ID type (string or number)
 * @template Filter - Filter type
 */
export abstract class QueryableResourceAPI<T, ID extends string | number, Filter = unknown>
  extends BaseResourceAPI
  implements QueryableResourceAPI<T, ID, Filter>
{
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

  /**
   * Apply filter criteria
   * @param resources Array of resources
   * @param filter Filter criteria
   * @returns Array of resources after filtering
   */
  protected applyFilter(resources: T[], filter: Filter): T[] {
    // Default implementation: return all resources without filtering
    // Subclasses can override this method to implement specific filtering logic
    void filter; // Explicitly acknowledge the parameter for linting purposes
    return resources;
  }

  // ============================================================================
  // Public Query API Methods - Simplified return types
  // ============================================================================

  /**
   * Get a single resource
   * @param id Resource ID
   * @returns Resource object or null if not found
   * @throws Error if operation fails
   */
  async get(id: ID): Promise<T | null> {
    return this.getResource(id);
  }

  /**
   * Get all resources
   * @param filter Optional filter criteria
   * @returns Array of resources
   * @throws Error if operation fails
   */
  async getAll(filter?: Filter): Promise<T[]> {
    let resources = await this.getAllResources();

    // Apply filter criteria
    if (filter) {
      resources = this.applyFilter(resources, filter);
    }

    return resources;
  }

  /**
   * Check if a resource exists
   * @param id Resource ID
   * @returns Boolean indicating existence
   * @throws Error if operation fails
   */
  async has(id: ID): Promise<boolean> {
    const resource = await this.getResource(id);
    return resource !== null;
  }

  /**
   * Get the count of resources
   * @returns Resource count
   * @throws Error if operation fails
   */
  async count(): Promise<number> {
    const resources = await this.getAllResources();
    return resources.length;
  }
}

// ============================================================================
// Improved CRUD Resource API - Semantic Separation
// ============================================================================

/**
 * Simplified CRUD Resource API with Semantic Separation (RECOMMENDED)
 *
 * Combines QueryableResourceAPI (simplified read operations) with WritableResourceAPI
 * (full error handling for write operations). This provides the best of both worlds:
 * - Simple, direct API for read operations (rarely fail)
 * - Comprehensive error handling for write operations (more likely to fail)
 *
 * Migration Guide:
 * If extending this class, implement:
 * 1. For read operations: getResource(id) and getAllResources()
 * 2. For write operations: createResource(), updateResource(), deleteResource()
 * 3. Optionally: clearResources(), validateResource(), validateUpdate()
 *
 * @template T - Resource type
 * @template ID - Resource ID type (string or number)
 * @template Filter - Filter type
 */
export abstract class SimplifiedCrudResourceAPI<T, ID extends string | number, Filter = unknown>
  extends QueryableResourceAPI<T, ID, Filter>
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

  /**
   * Check references for safe deletion (subclasses can override this method)
   * @param id Resource ID
   * @returns Delete check result
   */
  protected checkDeleteReferences(id: ID): Promise<DeleteCheckResult> | DeleteCheckResult {
    void id;
    return { canDelete: true, details: "No reference check configured", references: [] };
  }

  /**
   * Check if the resource can be safely deleted
   * @param id Resource ID
   * @returns Delete check result
   */
  async canSafelyDelete(id: ID): Promise<DeleteCheckResult> {
    return this.checkDeleteReferences(id);
  }

  /**
   * Delete with options (force flag to bypass reference check)
   * @param id Resource ID
   * @param options Delete options
   */
  async deleteWithOptions(id: ID, options?: { force?: boolean }): Promise<void> {
    if (!options?.force) {
      const check = await this.canSafelyDelete(id);
      if (!check.canDelete) {
        throw new ConfigurationValidationError(
          `Cannot delete resource '${id}': ${check.details}`,
          { configPath: "delete.referenced" },
        );
      }
    }
    await this.deleteResource(id);
  }

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
  // Public Write API Methods - Command Operations with Error Handling
  // ============================================================================

  /**
   * Create a new resource
   * @param resource Resource object
   * @returns Execution result with comprehensive error handling
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
   * @returns Execution result with comprehensive error handling
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
   * Delete a resource (with reference check by default)
   * @param id Resource ID
   * @returns Execution result with comprehensive error handling
   */
  async delete(id: ID): Promise<ExecutionResult<void>> {
    const startTime = now();

    try {
      await this.deleteWithOptions(id);
      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "DELETE", startTime);
    }
  }

  /**
   * Clear all resources
   * @returns Execution result with comprehensive error handling
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
