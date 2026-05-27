/**
 * Variable node processing function
 * Responsible for executing the VARIABLE node, evaluating variable expressions, and updating variable values.
 */

import type { RuntimeNode, VariableNodeConfig, EvaluationContext } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { expressionEvaluator, setArrayItemByKey } from "../../../evaluation/index.js";

/**
 * Evaluate the expression using ExpressionEvaluator (AST-based, safe)
 * Supports: comparison, logical, arithmetic, ternary operators, string methods, etc.
 */
function evaluateExpression(
  expression: string,
  _variableType: string,
  executionEntity: WorkflowExecutionEntity,
): unknown {
  try {
    // Handling empty string expressions
    if (!expression.trim()) {
      return "";
    }

    // Build evaluation context with all variables
    const allVariables = executionEntity.variableStateManager.getAllVariables();
    const input = executionEntity.getInput();
    const output = executionEntity.getOutput();

    const context: EvaluationContext = {
      variables: allVariables,
      input,
      output,
    };

    // Use AST-based evaluator (safe, preserves types)
    const result = expressionEvaluator.evaluate(expression, context);
    
    return result;
  } catch (error) {
    throw new RuntimeValidationError(`Failed to evaluate expression: ${expression}`, {
      operation: "handle",
      field: "variable.expression",
      context: {
        error: error instanceof Error ? error.message : String(error),
      },
    });
  }
}

/**
 * Type conversion
 */
function convertType(value: unknown, targetType: string): unknown {
  switch (targetType) {
    case "number": {
      const num = Number(value);
      // For invalid conversions such as the string "not a number", an error should be thrown.
      if (typeof value === "string" && value.trim() && isNaN(num)) {
        throw new RuntimeValidationError(`Failed to convert value to number: ${value}`, {
          operation: "handle",
          field: "variable.type",
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
          operation: "handle",
          field: "variable.type",
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
          operation: "handle",
          field: "variable.type",
        });
      }

    default:
      throw new RuntimeValidationError(`Invalid variable type: ${targetType}`, {
        operation: "handle",
        field: "variable.type",
      });
  }
}

/**
 * Variable Node Processing Function
 * @param executionEntity WorkflowExecutionEntity instance
 * @param node Node definition
 * @returns Execution result
 */
export async function variableHandler(
  executionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
): Promise<unknown> {
  const config = node.config as VariableNodeConfig;

  // Check if the variable is read-only.
  const workflowExecution = executionEntity.getExecution();
  const existingVariable = workflowExecution.variables?.find((v: { name: string; readonly?: boolean }) => v.name === config.variableName);
  if (existingVariable && existingVariable.readonly) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: executionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  // Evaluate the expression using AST-based evaluator (safe, preserves types)
  const result = evaluateExpression(config.expression, config.variableType, executionEntity);

  // Verify the type of the evaluation result.
  const typedResult = convertType(result, config.variableType);

  // Update the variable using setArrayItemByKey
  const updated = setArrayItemByKey(
    workflowExecution.variables as unknown as Record<string, unknown>[],
    "name",
    config.variableName,
    "value",
    typedResult,
  );
  if (!updated) {
    workflowExecution.variables.push({
      name: config.variableName,
      value: typedResult,
      type: config.variableType,
      readonly: config.readonly || false,
    });
  }

  // Use VariableManager to set the variable (handles all scope logic internally)
  executionEntity.variableStateManager.setVariable(config.variableName, typedResult);

  // Record execution history
  executionEntity.addNodeResult({
    step: executionEntity.getNodeResults().length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  // Return the execution results
  return {
    variableName: config.variableName,
    value: typedResult,
    type: config.variableType,
  };
}
