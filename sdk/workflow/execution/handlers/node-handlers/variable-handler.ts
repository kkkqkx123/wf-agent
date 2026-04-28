/**
 * Variable node processing function
 * Responsible for executing the VARIABLE node, evaluating variable expressions, and updating variable values.
 */

import type { Node, VariableNodeConfig, Thread } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { resolvePath } from "@wf-agent/common-utils";

/**
 * Check whether the node can be executed.
 */
function canExecute(threadEntity: ThreadEntity, node: Node): boolean {
  if (threadEntity.getStatus() !== "RUNNING") {
    return false;
  }

  const config = node.config as VariableNodeConfig;

  // Check if the variable is read-only.
  const thread = workflowExecutionEntity.getThread();
  const existingVariable = thread.variables?.find(v => v.name === config.variableName);
  if (existingVariable && existingVariable.readonly) {
    return false;
  }

  return true;
}

/**
 * Parse variable references in expressions
 * Use a unified path parsing logic
 */
function resolveVariableReferences(expression: string, thread: Thread): string {
  const variablePattern = /\{\{(\w+(?:\.\w+)*)\}\}/g;

  return expression.replace(variablePattern, (match, varPath) => {
    // Extract the root variable name.
    const rootVarName = varPath.split(".")[0];

    // First, try to obtain the value from the thread variable.
    let value: unknown = thread.variableScopes.thread?.[rootVarName];

    // If the first part does not exist in the thread variable, try to obtain it from the global variable.
    if (value === undefined && thread.variableScopes) {
      value = thread.variableScopes.global[rootVarName];
    }

    // If the root variable does not exist, return undefined.
    if (value === undefined) {
      return "undefined";
    }

    // If the path contains nesting, use resolvePath to parse the remaining path.
    const pathParts = varPath.split(".");
    if (pathParts.length > 1) {
      const remainingPath = pathParts.slice(1).join(".");
      value = resolvePath(remainingPath, value);
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
 * Evaluate the expression.
 */
function evaluateExpression(expression: string, variableType: string, thread: Thread): unknown {
  try {
    // Handling empty string expressions
    if (!expression.trim()) {
      return "";
    }

    // Create a function scope that includes variables from the thread scope.
    const threadScope = thread.variableScopes.thread || {};
    const globalScope = thread.variableScopes.global || {};

    const func = new Function(
      ...Object.keys({ ...threadScope, ...globalScope }),
      `return (${expression})`,
    );

    const result = func(...Object.values({ ...threadScope, ...globalScope }));
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
 * @param threadEntity ThreadEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function variableHandler(
  threadEntity: ThreadEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  // Check if it is possible to execute.
  if (!canExecute(threadEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: threadEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  const config = node.config as VariableNodeConfig;
  const thread = workflowExecutionEntity.getThread();

  // Parse variable references in expressions
  const evaluatedExpression = resolveVariableReferences(config.expression, thread);

  // Evaluate the expression.
  const result = evaluateExpression(evaluatedExpression, config.variableType, thread);

  // Verify the type of the evaluation result.
  const typedResult = convertType(result, config.variableType);

  // Update the variable
  const variable = thread.variables.find(v => v.name === config.variableName);
  const variableScope = config.scope || "thread";

  if (variable) {
    variable.value = typedResult;
  } else {
    thread.variables.push({
      name: config.variableName,
      value: typedResult,
      type: config.variableType,
      scope: variableScope,
      readonly: config.readonly || false,
    });
  }

  // Update variable values based on scope.
  switch (variableScope) {
    case "global":
      thread.variableScopes.global[config.variableName] = typedResult;
      break;
    case "thread":
      thread.variableScopes.thread[config.variableName] = typedResult;
      break;
    case "local":
      if (thread.variableScopes.local.length > 0) {
        thread.variableScopes.local[thread.variableScopes.local.length - 1]![config.variableName] =
          typedResult;
      }
      break;
    case "loop":
      if (thread.variableScopes.loop.length > 0) {
        thread.variableScopes.loop[thread.variableScopes.loop.length - 1]![config.variableName] =
          typedResult;
      }
      break;
  }

  // Record execution history
  threadEntity.addNodeResult({
    step: threadEntity.getNodeResults().length + 1,
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
