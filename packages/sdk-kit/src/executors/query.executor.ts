/**
 * Query Executor - Simplified execution queries
 */

import { ErrorConverter, KitError, KitErrorCode } from '../converters/error.converter.js';
import type { ExecutionRecord, FilterCriteria, SortOptions, PaginationOptions } from '../types/common.types.js';
import type {
  QueryBuilder,
  FilterExpression,
  AggregationOp,
  AggregationResult,
  ExportFormat,
} from '../types/query.types.js';

/**
 * Query Executor implementation
 */
export class QueryExecutor {
  private errorConverter: ErrorConverter;
  private sdk: any;

  constructor(sdk: any) {
    this.sdk = sdk;
    this.errorConverter = new ErrorConverter();
  }

  /**
   * Execute a query with filters, sort, and pagination
   */
  async query(
    filters?: FilterCriteria,
    sort?: SortOptions,
    pagination?: PaginationOptions
  ): Promise<ExecutionRecord[]> {
    try {
      // Get SDK factory
      const factory = this.sdk.getFactory?.();
      if (!factory) {
        throw new KitError(
          'SDK factory not available',
          KitErrorCode.INTERNAL_ERROR
        );
      }

      // Get execution registry
      const registry = factory.getWorkflowExecutionRegistry?.();
      if (!registry) {
        throw new KitError(
          'Execution registry not available',
          KitErrorCode.INTERNAL_ERROR
        );
      }

      // Convert filters to SDK format
      const sdkFilters = this.convertFilters(filters);

      // Execute query
      const results = await registry.query?.({
        filters: sdkFilters,
        sort,
        pagination,
      });

      // Convert results
      if (!results || !Array.isArray(results)) {
        return [];
      }

      return results.map((record) => this.convertRecord(record));
    } catch (error) {
      throw this.errorConverter.toKitError(error);
    }
  }

  /**
   * Apply advanced filter expressions
   */
  applyFilterExpressions(
    records: ExecutionRecord[],
    expressions: FilterExpression | FilterExpression[]
  ): ExecutionRecord[] {
    const exprs = Array.isArray(expressions) ? expressions : [expressions];

    return records.filter((record) =>
      exprs.every((expr) => this.evaluateExpression(record, expr))
    );
  }

  /**
   * Evaluate a single filter expression
   */
  private evaluateExpression(record: ExecutionRecord, expr: FilterExpression): boolean {
    const value = this.getFieldValue(record, expr.field);

    switch (expr.operator) {
      case 'eq':
        return value === expr.value;
      case 'neq':
        return value !== expr.value;
      case 'gt':
        return value > expr.value;
      case 'gte':
        return value >= expr.value;
      case 'lt':
        return value < expr.value;
      case 'lte':
        return value <= expr.value;
      case 'in':
        return Array.isArray(expr.value) && expr.value.includes(value);
      case 'nin':
        return !Array.isArray(expr.value) || !expr.value.includes(value);
      case 'contains':
        return String(value).includes(String(expr.value));
      case 'regex':
        return new RegExp(expr.value).test(String(value));
      default:
        return true;
    }
  }

  /**
   * Get nested field value from record
   */
  private getFieldValue(record: any, field: string): any {
    const parts = field.split('.');
    let value = record;

    for (const part of parts) {
      if (value && typeof value === 'object') {
        value = value[part];
      } else {
        return undefined;
      }
    }

    return value;
  }

  /**
   * Apply aggregation operations
   */
  async aggregate(
    records: ExecutionRecord[],
    operations: AggregationOp | AggregationOp[]
  ): Promise<AggregationResult[]> {
    const ops = Array.isArray(operations) ? operations : [operations];
    const results: AggregationResult[] = [];

    for (const op of ops) {
      const result = this.performAggregation(records, op);
      results.push(result);
    }

    return results;
  }

  /**
   * Perform a single aggregation operation
   */
  private performAggregation(records: ExecutionRecord[], op: AggregationOp): AggregationResult {
    const result: AggregationResult = {};

    if (op.type === 'count') {
      result[op.as || 'count'] = records.length;
    } else if (op.type === 'sum' && op.field) {
      const sum = records.reduce((acc, rec) => {
        const val = this.getFieldValue(rec, op.field!);
        return acc + (typeof val === 'number' ? val : 0);
      }, 0);
      result[op.as || 'sum'] = sum;
    } else if (op.type === 'avg' && op.field) {
      const sum = records.reduce((acc, rec) => {
        const val = this.getFieldValue(rec, op.field!);
        return acc + (typeof val === 'number' ? val : 0);
      }, 0);
      result[op.as || 'avg'] = records.length > 0 ? sum / records.length : 0;
    } else if (op.type === 'min' && op.field) {
      const values = records
        .map((rec) => this.getFieldValue(rec, op.field!))
        .filter((v) => typeof v === 'number');
      result[op.as || 'min'] = Math.min(...values);
    } else if (op.type === 'max' && op.field) {
      const values = records
        .map((rec) => this.getFieldValue(rec, op.field!))
        .filter((v) => typeof v === 'number');
      result[op.as || 'max'] = Math.max(...values);
    } else if (op.type === 'group_by' && op.groupBy) {
      const groups: Record<string, number> = {};
      records.forEach((rec) => {
        const key = String(this.getFieldValue(rec, op.groupBy!));
        groups[key] = (groups[key] || 0) + 1;
      });
      result[op.as || 'groups'] = groups;
    }

    return result;
  }

  /**
   * Export records to specified format
   */
  exportToFormat(records: ExecutionRecord[], format: ExportFormat): string {
    switch (format) {
      case 'json':
        return JSON.stringify(records, null, 2);
      case 'csv':
        return this.exportToCSV(records);
      case 'xml':
        return this.exportToXML(records);
      case 'parquet':
        // Parquet is complex - return JSON for now
        return JSON.stringify(records, null, 2);
      default:
        return JSON.stringify(records, null, 2);
    }
  }

  /**
   * Export to CSV format
   */
  private exportToCSV(records: ExecutionRecord[]): string {
    if (records.length === 0) return '';

    const firstRecord = records[0];
    if (!firstRecord) return '';

    const headers = Object.keys(firstRecord);
    const rows = records.map((record) =>
      headers
        .map((header) => {
          const value = (record as any)[header];
          if (typeof value === 'string' && value.includes(',')) {
            return `"${value}"`;
          }
          return value;
        })
        .join(',')
    );

    return [headers.join(','), ...rows].join('\n');
  }

  /**
   * Export to XML format
   */
  private exportToXML(records: ExecutionRecord[]): string {
    let xml = '<?xml version="1.0" encoding="UTF-8"?>\n<records>\n';

    for (const record of records) {
      xml += '  <record>\n';
      for (const [key, value] of Object.entries(record)) {
        if (value !== undefined && value !== null) {
          xml += `    <${key}>${this.escapeXML(String(value))}</${key}>\n`;
        }
      }
      xml += '  </record>\n';
    }

    xml += '</records>';
    return xml;
  }

  /**
   * Escape XML special characters
   */
  private escapeXML(str: string): string {
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&apos;');
  }

  /**
   * Get distinct values for a field
   */
  getDistinct(records: ExecutionRecord[], field: string): any[] {
    const values = records
      .map((rec) => this.getFieldValue(rec, field))
      .filter((v) => v !== undefined);

    return Array.from(new Set(values));
  }

  /**
   * Group records by a field
   */
  groupByField(records: ExecutionRecord[], field: string): Map<any, ExecutionRecord[]> {
    const groups = new Map<any, ExecutionRecord[]>();

    for (const record of records) {
      const key = this.getFieldValue(record, field);
      if (!groups.has(key)) {
        groups.set(key, []);
      }
      groups.get(key)!.push(record);
    }

    return groups;
  }

  /**
   * Convert high-level filter criteria to SDK format
   */
  private convertFilters(filters?: FilterCriteria): any {
    if (!filters) return {};

    return {
      workflowId: filters.workflowId,
      status: filters.status,
      createdAfter: filters.startTime?.from,
      createdBefore: filters.startTime?.to,
      tags: filters.tags,
      ...filters.custom,
    };
  }

  /**
   * Convert SDK execution record to Kit format
   */
  private convertRecord(record: any): ExecutionRecord {
    const startTime = record.createdAt || record.startTime || Date.now();
    const endTime = record.completedAt || record.endTime;

    return {
      executionId: record.id || record.executionId || '',
      workflowId: record.workflowId || '',
      status: record.status || 'pending',
      input: record.input,
      output: record.output,
      error: record.error?.message || record.error,
      startTime,
      endTime,
      duration: endTime ? endTime - startTime : undefined,
    };
  }
}

/**
 * Query Builder implementation with advanced features
 */
export class QueryBuilderImpl implements QueryBuilder {
  private filters?: FilterCriteria;
  private filterExpressions: FilterExpression[] = [];
  private sortOptions?: SortOptions;
  private pagination: PaginationOptions = { limit: 100, offset: 0 };

  constructor(private executor: QueryExecutor) {}

  filter(criteria: FilterCriteria): QueryBuilder {
    this.filters = { ...this.filters, ...criteria };
    return this;
  }

  filterBy(expressions: FilterExpression | FilterExpression[]): QueryBuilder {
    if (Array.isArray(expressions)) {
      this.filterExpressions.push(...expressions);
    } else {
      this.filterExpressions.push(expressions);
    }
    return this;
  }

  sort(field: string, order: 'asc' | 'desc'): QueryBuilder {
    this.sortOptions = { field, order };
    return this;
  }

  limit(count: number): QueryBuilder {
    this.pagination.limit = count;
    return this;
  }

  offset(count: number): QueryBuilder {
    this.pagination.offset = count;
    return this;
  }

  async get(): Promise<ExecutionRecord[]> {
    let results = await this.executor.query(this.filters, this.sortOptions, this.pagination);

    // Apply advanced filter expressions
    if (this.filterExpressions.length > 0) {
      results = this.executor.applyFilterExpressions(results, this.filterExpressions);
    }

    return results;
  }

  async first(): Promise<ExecutionRecord | null> {
    const results = await this.executor.query(
      this.filters,
      this.sortOptions,
      { limit: 1, offset: 0 }
    );

    // Apply advanced filters if any
    if (this.filterExpressions.length > 0) {
      const filtered = this.executor.applyFilterExpressions(results, this.filterExpressions);
      return filtered[0] || null;
    }

    return results[0] || null;
  }

  async count(): Promise<number> {
    const results = await this.executor.query(this.filters);

    // Apply advanced filters if any
    if (this.filterExpressions.length > 0) {
      const filtered = this.executor.applyFilterExpressions(results, this.filterExpressions);
      return filtered.length;
    }

    return results.length;
  }

  async aggregate(operations: AggregationOp | AggregationOp[]): Promise<AggregationResult[]> {
    const records = await this.get();
    return this.executor.aggregate(records, operations);
  }

  async export(format: ExportFormat): Promise<string> {
    const records = await this.get();
    return this.executor.exportToFormat(records, format);
  }

  async distinct(field: string): Promise<any[]> {
    const records = await this.get();
    return this.executor.getDistinct(records, field);
  }

  async groupBy(field: string): Promise<Map<any, ExecutionRecord[]>> {
    const records = await this.get();
    return this.executor.groupByField(records, field);
  }
}

/**
 * Query API implementation
 */
export class QueryAPIImpl {
  constructor(private executor: QueryExecutor) {}

  executions(): QueryBuilder {
    return new QueryBuilderImpl(this.executor);
  }
}
