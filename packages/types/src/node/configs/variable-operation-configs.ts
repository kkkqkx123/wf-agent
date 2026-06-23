/**
 * Variable Operation Configuration Types
 *
 * Supports operations on VariableStateManager for:
 * - Aggregating multiple variables
 * - Transforming variable values
 * - Batch updating multiple variables
 *
 * These operations target internal workflow runtime variables, not external data.
 */

/**
 * Filter expression for conditional aggregation
 * When aggregating arrays or objects, can filter elements before aggregation
 */
export interface FilterExpression {
  /** Expression to evaluate (AST-based) */
  expression: string;
  /** Type of value being filtered: 'array' | 'object' */
  valueType?: 'array' | 'object';
}

/**
 * Aggregate multiple variables into a single variable
 * Supports three modes: array, object, merge
 */
export interface VariableAggregateOperation {
  operation: 'aggregate';

  /** Source variable names to aggregate */
  sourceVariables: string[];

  /** Target variable name to store aggregated result */
  targetVariable: string;

  /** Aggregation mode */
  aggregateMode: 'array' | 'object' | 'merge';

  /**
   * For 'object' mode: maps source variable names to output keys
   * Example: { "analysis_sentiment": "sentiment", "analysis_intent": "intent" }
   * If not provided, uses source variable names as keys
   */
  keyMapping?: Record<string, string>;

  /**
   * Optional filter expression when source variables contain arrays
   * Applied before aggregation
   * Example: { expression: "item.status === 'success'" }
   */
  filterExpression?: FilterExpression;

  /**
   * For 'merge' mode: specify merge strategy
   * 'shallow': shallow merge (only top level)
   * 'deep': deep merge (recursive)
   */
  mergeStrategy?: 'shallow' | 'deep';
}

/**
 * Transform a variable's value
 * Supports expression-based transformation
 */
export interface VariableTransformOperation {
  operation: 'transform';

  /** Source variable name */
  sourceVariable: string;

  /** Target variable name */
  targetVariable: string;

  /** Transformation expression (AST-based evaluation) */
  transformExpression: string;

  /** Optional output type for type conversion */
  outputType?: 'string' | 'number' | 'boolean' | 'array' | 'object';
}

/**
 * Update multiple variables atomically
 * Each update is evaluated as an expression with access to all variables
 */
export interface VariableBatchUpdateOperation {
  operation: 'batch-update';

  /** List of variable updates */
  updates: Array<{
    /** Variable name to update */
    name: string;

    /** Expression to evaluate (has access to all variables) */
    expression: string;

    /** Optional target type for type conversion */
    type?: 'string' | 'number' | 'boolean' | 'array' | 'object';

    /** Whether this variable is read-only (inherited from variable definition) */
    readonly?: boolean;
  }>;
}

/**
 * Union type for all variable operations
 */
export type VariableOperationConfig =
  | VariableAggregateOperation
  | VariableTransformOperation
  | VariableBatchUpdateOperation;

/**
 * Output from a variable operation
 */
export interface VariableOperationOutput {
  operation: string;

  /** Variables that were modified */
  modifiedVariables: Array<{
    name: string;
    newValue: unknown;
    type?: string;
  }>;

  /** Execution time in milliseconds */
  executionTime: number;

  /** Operation statistics */
  stats?: {
    sourceVariableCount?: number;
    aggregatedItemCount?: number;
  };
}
