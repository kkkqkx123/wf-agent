/**
 * Variable Operation Handlers
 *
 * Implements execution logic for variable operations:
 * - aggregate: Combine multiple variables
 * - transform: Transform a variable's value
 * - batch-update: Update multiple variables atomically
 */

import type {
  VariableAggregateOperation,
  VariableTransformOperation,
  VariableBatchUpdateOperation,
  EvaluationContext,
} from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { expressionEvaluator, setArrayItemByKey } from "../../../../services/evaluation/index.js";
import type { VariableManager } from "../../utils/variable-manager.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Type conversion utility (reused from variable-handler.ts)
 */
function convertType(value: unknown, targetType?: string): unknown {
  if (!targetType) {
    return value;
  }

  switch (targetType) {
    case "number": {
      const num = Number(value);
      if (typeof value === "string" && value.trim() && isNaN(num)) {
        throw new RuntimeValidationError(`Failed to convert value to number: ${value}`, {
          operation: "variable-operation",
          field: "type-conversion",
          value,
        });
      }
      return num;
    }

    case "string":
      return String(value);

    case "boolean":
      return Boolean(value);

    case "array":
      if (Array.isArray(value)) {
        return value;
      }
      try {
        return Array.from(value as Iterable<unknown>);
      } catch {
        throw new RuntimeValidationError(`Failed to convert value to array: ${value}`, {
          operation: "variable-operation",
          field: "type-conversion",
          value,
        });
      }

    case "object":
      if (typeof value === "object" && value !== null && !Array.isArray(value)) {
        return value;
      }
      if (value === null) {
        return null;
      }
      try {
        return Object(value);
      } catch {
        throw new RuntimeValidationError(`Failed to convert value to object: ${value}`, {
          operation: "variable-operation",
          field: "type-conversion",
        });
      }

    default:
      throw new RuntimeValidationError(`Invalid type: ${targetType}`, {
        operation: "variable-operation",
        field: "type",
      });
  }
}

/**
 * Apply filter expression to array elements
 */
function applyFilter(items: unknown[], filterExpression: string): unknown[] {
  if (!Array.isArray(items)) {
    throw new RuntimeValidationError(
      `Cannot apply filter to non-array value`,
      {
        operation: "variable-operation",
        field: "filter",
      }
    );
  }

  // Create evaluation context where 'item' is the current array element
  return items.filter((item) => {
    const context: EvaluationContext = { variables: { item }, input: {} as Record<string, unknown>, output: {} as Record<string, unknown> };
    try {
      const result = expressionEvaluator.evaluate(filterExpression, context);
      return Boolean(result);
    } catch (error) {
      throw new RuntimeValidationError(
        `Failed to evaluate filter expression: ${filterExpression}`,
        {
          operation: "variable-operation",
          field: "filter-expression",
          context: {
            error: error instanceof Error ? error.message : String(error)
          },
        }
      );
    }
  });
}

/**
 * Execute aggregate operation
 */
export function executeAggregate(
  operation: VariableAggregateOperation,
  variableManager: VariableManager,
  _allVariables: Record<string, unknown>,
): { value: unknown; modified: Array<{ name: string; newValue: unknown }> } {
  // Verify all source variables exist
  const sourceValues: Record<string, unknown> = {};
  for (const varName of operation.sourceVariables) {
    const value = variableManager.getVariable(varName);
    if (value === undefined) {
      throw new RuntimeValidationError(
        `Source variable not found: ${varName}`,
        {
          operation: "aggregate",
          field: "sourceVariables",
        }
      );
    }
    sourceValues[varName] = value;
  }

  let aggregatedValue: unknown;

  if (operation.aggregateMode === "array") {
    // Array mode: collect values into an array
    const items = Object.values(sourceValues);

    // Apply filter if specified
    if (operation.filterExpression) {
      aggregatedValue = items
        .filter((item) => Array.isArray(item))
        .flatMap((item) =>
          applyFilter(item as unknown[], operation.filterExpression!.expression)
        );
    } else {
      aggregatedValue = items;
    }
  } else if (operation.aggregateMode === "object") {
    // Object mode: create an object with mapped keys
    const result: Record<string, unknown> = {};

    for (const varName of operation.sourceVariables) {
      const key = operation.keyMapping?.[varName] || varName;
      result[key] = sourceValues[varName];
    }

    aggregatedValue = result;
  } else if (operation.aggregateMode === "merge") {
    // Merge mode: merge objects
    const strategy = operation.mergeStrategy || "shallow";
    let merged: Record<string, unknown> | unknown[] = {};

    for (const varName of operation.sourceVariables) {
      const value = sourceValues[varName];

      if (typeof value === "object" && value !== null && !Array.isArray(value)) {
        if (strategy === "shallow") {
          merged = { ...merged as Record<string, unknown>, ...(value as Record<string, unknown>) };
        } else {
          // Deep merge (simple implementation)
          merged = deepMerge(merged as Record<string, unknown>, value as Record<string, unknown>);
        }
      } else {
        throw new RuntimeValidationError(
          `Cannot merge non-object value from variable: ${varName}`,
          {
            operation: "aggregate",
            field: "mergeMode",
          }
        );
      }
    }

    aggregatedValue = merged;
  }

  // Update target variable
  variableManager.setVariable(operation.targetVariable, aggregatedValue);

  return {
    value: aggregatedValue,
    modified: [
      {
        name: operation.targetVariable,
        newValue: aggregatedValue,
      },
    ],
  };
}

/**
 * Simple deep merge utility
 */
function deepMerge(target: Record<string, unknown> | unknown[], source: Record<string, unknown> | unknown[]): Record<string, unknown> | unknown[] {
  if (!source || typeof source !== "object" || Array.isArray(source)) {
    return source;
  }

  const result = Array.isArray(target) ? [...target] : { ...target };

  for (const key in source) {
    if (Object.prototype.hasOwnProperty.call(source, key)) {
      const sourceValue = (source as Record<string, unknown>)[key];
      const resultValue = (result as Record<string, unknown>)[key];
      if (
        typeof sourceValue === "object" &&
        sourceValue !== null &&
        !Array.isArray(sourceValue) &&
        resultValue &&
        typeof resultValue === "object" &&
        !Array.isArray(resultValue)
      ) {
        (result as Record<string, unknown>)[key] = deepMerge(resultValue as Record<string, unknown>, sourceValue as Record<string, unknown>);
      } else {
        (result as Record<string, unknown>)[key] = sourceValue;
      }
    }
  }

  return result;
}

/**
 * Execute transform operation
 */
export function executeTransform(
  operation: VariableTransformOperation,
  variableManager: VariableManager,
  allVariables: Record<string, unknown>,
): { value: unknown; modified: Array<{ name: string; newValue: unknown }> } {
  // Get source variable
  const sourceValue = variableManager.getVariable(operation.sourceVariable);
  if (sourceValue === undefined) {
    throw new RuntimeValidationError(
      `Source variable not found: ${operation.sourceVariable}`,
      {
        operation: "transform",
        field: "sourceVariable",
      }
    );
  }

  // Evaluate transform expression
  try {
    const context = { variables: allVariables };
    const transformed = expressionEvaluator.evaluate(
      operation.transformExpression,
      context as EvaluationContext
    );

    // Type conversion if specified
    const typedValue = convertType(transformed, operation.outputType);

    // Update target variable
    variableManager.setVariable(operation.targetVariable, typedValue);

    return {
      value: typedValue,
      modified: [
        {
          name: operation.targetVariable,
          newValue: typedValue,
        },
      ],
    };
  } catch (error) {
    throw new RuntimeValidationError(
      `Failed to evaluate transform expression: ${operation.transformExpression}`,
      {
        operation: "transform",
        field: "transformExpression",
        context: {
          error: error instanceof Error ? error.message : String(error)
        },
      }
    );
  }
}

/**
 * Execute batch update operation
 */
export function executeBatchUpdate(
  operation: VariableBatchUpdateOperation,
  variableManager: VariableManager,
  allVariables: Record<string, unknown>,
  workflowExecution?: Record<string, unknown>,
): { modified: Array<{ name: string; newValue: unknown }> } {
  const modified: Array<{ name: string; newValue: unknown }> = [];

  for (const update of operation.updates) {
    // Check read-only
    if (update.readonly) {
      logger.debug("Skipping read-only variable", { variableName: update.name });
      continue;
    }

    // Evaluate expression
    try {
      const context = { variables: allVariables };
      const value = expressionEvaluator.evaluate(update.expression, context as EvaluationContext);

      // Type conversion if specified
      const typedValue = convertType(value, update.type);

      // Update variable and workflow execution variables array
      variableManager.setVariable(update.name, typedValue);

      // Also update the workflow execution's variables array if available
      if (workflowExecution?.['variables']) {
        const wfVars = workflowExecution['variables'] as Array<{ name: string; value: unknown; type?: string; readonly: boolean }>;
        const updated = setArrayItemByKey(
          wfVars,
          "name",
          update.name,
          "value",
          typedValue
        );
        if (!updated) {
          (workflowExecution['variables'] as Array<any>).push({
            name: update.name,
            value: typedValue,
            type: update.type,
            readonly: false,
          });
        }
      }

      modified.push({
        name: update.name,
        newValue: typedValue,
      });

      // Update allVariables reference for subsequent evaluations
      allVariables[update.name] = typedValue;
    } catch (error) {
      throw new RuntimeValidationError(
        `Failed to evaluate batch update expression for variable ${update.name}: ${update.expression}`,
        {
          operation: "batch-update",
          field: "expression",
          context: {
            error: error instanceof Error ? error.message : String(error)
          },
        }
      );
    }
  }

  return { modified };
}
