/**
 * Variable node processing function
 * Responsible for executing the VARIABLE node, evaluating variable expressions, and updating variable values.
 */

import type { RuntimeNode, VariableNodeConfig, WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { resolvePath } from "@wf-agent/common-utils";
import { VariableAccessor } from "../../utils/variable-accessor.js";

/**
 * Check whether the node can be executed.
 */
function canExecute(executionEntity: WorkflowExecutionEntity, node: RuntimeNode): boolean {
  if (executionEntity.getStatus() !== "RUNNING") {
    return false;
  }

  const config = node.config as VariableNodeConfig;

  // Check if the variable is read-only.
  const workflowExecution = executionEntity.getExecution();
  const existingVariable = workflowExecution.variables?.find((v: { name: string; readonly?: boolean }) => v.name === config.variableName);
  if (existingVariable && existingVariable.readonly) {
    return false;
  }

  return true;
}

/**
 * Parse variable references in expressions
 * Use VariableAccessor for unified path parsing logic
 */
function resolveVariableReferences(
  expression: string,
  executionEntity: WorkflowExecutionEntity,
): string {
  const variablePattern = /\{\{(\w+(?:\.\w+)*)\}\}/g;
  const accessor = new VariableAccessor(executionEntity);

  return expression.replace(variablePattern, (match, varPath) => {
    // Use VariableAccessor to get the value (handles all scopes)
    const value = accessor.get(varPath);

    // If the variable does not exist, return undefined.
    if (value === undefined) {
      return "undefined";
    }

    // Formatted values
    if (typeof value === "string") {
      return `'${value}'`;
    } else if (typeof value === "object") {
      return JSON.stringify(value);
    } else {
      return String(value);
    }
  });
}

/**
 * Evaluate the expression using VariableManager's variables
 */
function evaluateExpression(
  expression: string,
  variableType: string,
  executionEntity: WorkflowExecutionEntity,
): unknown {
  try {
    // Handling empty string expressions
    if (!expression.trim()) {
      return "";
    }

    // Get all variables from VariableManager (includes all scopes)
    const allVariables = executionEntity.variableStateManager.getAllVariables();

    const func = new Function(...Object.keys(allVariables), `return (${expression})`);
    const result = func(...Object.values(allVariables));
    return result;
  } catch {
    throw new RuntimeValidationError(`Failed to evaluate expression: ${expression}`, {
      operation: "handle",
      field: "variable.expression",
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
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function variableHandler(
  executionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  _context?: unknown,
): Promise<unknown> {
  // Check if it is possible to execute.
  if (!canExecute(executionEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: executionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  const config = node.config as VariableNodeConfig;
  const workflowExecution = executionEntity.getExecution();

  // Parse variable references in expressions
  const evaluatedExpression = resolveVariableReferences(config.expression, executionEntity);

  // Evaluate the expression.
  const result = evaluateExpression(evaluatedExpression, config.variableType, executionEntity);

  // Verify the type of the evaluation result.
  const typedResult = convertType(result, config.variableType);

  // Update the variable using VariableManager
  const variable = workflowExecution.variables.find((v) => v.name === config.variableName);
  const variableScope = config.scope || "execution";

  if (variable) {
    variable.value = typedResult;
  } else {
    workflowExecution.variables.push({
      name: config.variableName,
      value: typedResult,
      type: config.variableType,
      scope: variableScope,
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
