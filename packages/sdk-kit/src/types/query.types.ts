/**
 * Query-related types for SDK-Kit
 */

import type { ExecutionRecord, FilterCriteria } from './common.types.js';

/**
 * Advanced filter expression
 */
export interface FilterExpression {
  field: string;
  operator: 'eq' | 'neq' | 'gt' | 'gte' | 'lt' | 'lte' | 'in' | 'nin' | 'contains' | 'regex';
  value: any;
}

/**
 * Aggregation operation
 */
export interface AggregationOp {
  type: 'count' | 'sum' | 'avg' | 'min' | 'max' | 'group_by';
  field?: string;
  groupBy?: string;
  as?: string;
}

/**
 * Aggregation result
 */
export interface AggregationResult {
  [key: string]: any;
}

/**
 * Export format options
 */
export type ExportFormat = 'json' | 'csv' | 'xml' | 'parquet';

/**
 * Query builder interface with advanced features
 */
export interface QueryBuilder {
  /**
   * Apply basic filter criteria
   */
  filter(criteria: FilterCriteria): QueryBuilder;

  /**
   * Apply advanced filter expression
   */
  filterBy(expressions: FilterExpression | FilterExpression[]): QueryBuilder;

  /**
   * Sort results
   */
  sort(field: string, order: 'asc' | 'desc'): QueryBuilder;

  /**
   * Limit number of results
   */
  limit(count: number): QueryBuilder;

  /**
   * Offset results
   */
  offset(count: number): QueryBuilder;

  /**
   * Apply aggregation operations
   */
  aggregate(operations: AggregationOp | AggregationOp[]): Promise<AggregationResult[]>;

  /**
   * Execute query and return results
   */
  get(): Promise<ExecutionRecord[]>;

  /**
   * Get first result
   */
  first(): Promise<ExecutionRecord | null>;

  /**
   * Get total count
   */
  count(): Promise<number>;

  /**
   * Export results to specified format
   */
  export(format: ExportFormat): Promise<string>;

  /**
   * Get results as distinct values for a field
   */
  distinct(field: string): Promise<any[]>;

  /**
   * Group results by a field
   */
  groupBy(field: string): Promise<Map<any, ExecutionRecord[]>>;
}

