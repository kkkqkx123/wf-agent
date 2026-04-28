/**
 * Core Interface for Query Mode
 * Defines a unified interface for pure query operations
 */

import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Query Metadata
 */
export interface QueryMetadata {
  /** Query name */
  name: string;
  /** Query description */
  description: string;
  /** Query category */
  category: "checkpoints" | "triggers" | "events" | "messages" | "state" | "batch";
  /** Is authentication required? */
  requiresAuth: boolean;
  /** Query version */
  version: string;
}

/**
 * Query results
 */
export type QueryResult<T> =
  | { success: true; data: T; executionTime: number }
  | { success: false; error: string; executionTime: number };

/**
 * Query results were successfully created.
 */
export function querySuccess<T>(data: T, executionTime: number): QueryResult<T> {
  return { success: true, data, executionTime };
}

/**
 * Creation of the failed query result failed.
 */
export function queryFailure<T>(error: string, executionTime: number): QueryResult<T> {
  return { success: false, error, executionTime };
}

/**
 * Check whether the query results were successful.
 */
export function isQuerySuccess<T>(
  result: QueryResult<T>,
): result is { success: true; data: T; executionTime: number } {
  return result.success === true;
}

/**
 * Check whether the query results have failed.
 */
export function isQueryFailure<T>(
  result: QueryResult<T>,
): result is { success: false; error: string; executionTime: number } {
  return result.success === false;
}

/**
 * Query Interface
 * All query operations must implement this interface.
 */
export interface Query<T> {
  /**
   * Execute the query
   * @returns The query results
   */
  execute(): Promise<QueryResult<T>>;

  /**
   * Retrieve query metadata
   * @returns Query metadata
   */
  getMetadata(): QueryMetadata;
}

/**
 * Abstract Query Base Class
 * Provides a generic implementation for queries
 */
export abstract class BaseQuery<T> implements Query<T> {
  protected readonly startTime: number = now();

  /**
   * Execute the query
   */
  abstract execute(): Promise<QueryResult<T>>;

  /**
   * Retrieve query metadata
   */
  abstract getMetadata(): QueryMetadata;

  /**
   * Get execution time
   */
  protected getExecutionTime(): number {
    return diffTimestamp(this.startTime, now());
  }
}
