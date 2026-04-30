/**
 * LoopStart node processing function
 * Responsible for executing the LOOP_START node, initializing loop variables, and setting loop conditions.
 */

import type { Node, LoopStartNodeConfig } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { ExecutionError, ValidationError, RuntimeValidationError } from "@wf-agent/types";
import { now, getErrorMessage } from "@wf-agent/common-utils";

/**
 * Loop state
 */
interface LoopState {
  loopId: string;
  iterable: unknown | null; // Can be null (during counting loops).
  currentIndex: number;
  maxIterations: number;
  iterationCount: number;
  variableName: string | null; // Can be null (when counting loops).
}

/**
 * Check if the node can be executed.
 */
function canExecute(executionEntity: WorkflowExecutionEntity, node: Node): boolean {
  if (executionEntity.getStatus() !== "RUNNING") {
    return false;
  }

  const thread = executionEntity.getExecution();
  const config = node.config as LoopStartNodeConfig;
  const loopState = getLoopState(thread);

  // If the loop state does not exist, it can be executed (for the first time).
  if (!loopState) {
    return true;
  }

  // If loop state exists, execution is always allowed
  // Whether the loop continues is judged internally by the handler
  return true;
}

/**
 * Verify if the iterable is valid.
 */
function isValidIterable(iterable: unknown): boolean {
  return (
    Array.isArray(iterable) ||
    (typeof iterable === "object" && iterable !== null) ||
    typeof iterable === "number" ||
    typeof iterable === "string"
  );
}

/**
 * Parsing iterable: support for direct values or variable expressions
 *
 * Supported formats:
 * - Direct values: [1,2,3], {a:1}, 5, "hello"
 * - Variable expressions: {{input.list}}, {{thread.items}}, {{global.data}}
 */
function resolveIterable(iterableConfig: unknown, thread: WorkflowExecution): unknown {
  // If no iterable configuration is provided, return null (counting loop mode).
  if (iterableConfig === undefined || iterableConfig === null) {
    return null;
  }

  // If it is a string, check whether it is a variable expression.
  if (typeof iterableConfig === "string") {
    const varExprPattern = /^\{\{([\w.]+)\}\}$/;
    const match = iterableConfig.match(varExprPattern);

    if (match && match[1]) {
      // This is a variable expression that needs to be parsed from a thread.
      const varPath = match[1];
      const parts = varPath.split(".");
      const scope = parts[0];

      try {
        let value: unknown;

        // Get variables based on their scope.
        switch (scope) {
          case "input":
            value = thread.input;
            // Parse nested paths
            for (let i = 1; i < parts.length; i++) {
              value = (value as Record<string, unknown>)?.[parts[i]!];
            }
            break;

          case "output":
            value = thread.output;
            for (let i = 1; i < parts.length; i++) {
              value = (value as Record<string, unknown>)?.[parts[i]!];
            }
            break;

          case "global":
            value = thread.variableScopes.global;
            for (let i = 1; i < parts.length; i++) {
              value = (value as Record<string, unknown>)?.[parts[i]!];
            }
            break;

          case "workflowExecution":
            value = thread.variableScopes.workflowExecution;
            for (let i = 1; i < parts.length; i++) {
              value = (value as Record<string, unknown>)?.[parts[i]!];
            }
            break;

          default:
            throw new RuntimeValidationError(
              `Invalid variable scope '${scope}'. Supported scopes: input, output, global, thread`,
              {
                operation: "handle",
                field: "loop.scope",
                value: scope,
              },
            );
        }

        if (value === undefined) {
          throw new ExecutionError(
            `Variable '${varPath}' not found in thread context`,
            thread.currentNodeId,
            thread.workflowId,
            { varPath, iterableConfig },
          );
        }

        return value;
      } catch (error) {
        if (error instanceof ExecutionError || error instanceof ValidationError) {
          throw error;
        }
        throw new ExecutionError(
          `Failed to resolve iterable expression '${iterableConfig}': ${getErrorMessage(error)}`,
          thread.currentNodeId,
          thread.workflowId,
          { iterableConfig },
        );
      }
    }
  }

  // Direct values, type validation
  if (!isValidIterable(iterableConfig)) {
    throw new RuntimeValidationError(
      `Iterable must be an array, object, number, string, or variable expression like {{input.list}}. Got: ${typeof iterableConfig}`,
      {
        operation: "handle",
        field: "loop.iterable",
        value: iterableConfig,
      },
    );
  }

  return iterableConfig;
}

/**
 * Get the loop status
 */
function getLoopState(thread: WorkflowExecution): LoopState | undefined {
  const currentLoopScope = thread.variableScopes.loop[thread.variableScopes.loop.length - 1] as
    | Record<string, unknown>
    | undefined;
  if (currentLoopScope) {
    return currentLoopScope[`__loop_state`] as LoopState | undefined;
  }
  return undefined;
}

/**
 * Set the loop state within the loop scope.
 */
function setLoopState(thread: WorkflowExecution, loopState: LoopState): void {
  const currentLoopScope = thread.variableScopes.loop[thread.variableScopes.loop.length - 1];
  if (currentLoopScope) {
    currentLoopScope[`__loop_state`] = loopState;
  }
}

/**
 * Clear the loop state (only delete the state objects; the scope is cleared upon exit).
 */
function clearLoopState(thread: WorkflowExecution): void {
  const currentLoopScope = thread.variableScopes.loop[thread.variableScopes.loop.length - 1];
  if (currentLoopScope) {
    delete currentLoopScope[`__loop_state`];
  }
}

/**
 * Check the loop condition.
 */
function checkLoopCondition(loopState: LoopState): boolean {
  // Check if maxIterations is valid (it must be a positive number).
  if (loopState.maxIterations <= 0) {
    return false;
  }

  // Check the number of iterations.
  if (loopState.iterationCount >= loopState.maxIterations) {
    return false;
  }

  // If an iterable is provided, it is also necessary to check whether the current index is out of range.
  if (loopState.iterable !== null && loopState.iterable !== undefined) {
    const iterableLength = getIterableLength(loopState.iterable);
    if (loopState.currentIndex >= iterableLength) {
      return false;
    }
  }

  return true;
}

/**
 * Get the length of the iterable.
 */
function getIterableLength(iterable: unknown): number {
  if (Array.isArray(iterable)) {
    return iterable.length;
  } else if (typeof iterable === "object" && iterable !== null) {
    return Object.keys(iterable).length;
  } else if (typeof iterable === "number") {
    return iterable;
  } else if (typeof iterable === "string") {
    return iterable.length;
  }
  return 0;
}

/**
 * Get the current iteration value
 */
function getCurrentValue(loopState: LoopState): unknown {
  const { iterable, currentIndex } = loopState;

  // If there is no iterable (for counting loops), return the current index.
  if (iterable === null || iterable === undefined) {
    return currentIndex;
  }

  if (Array.isArray(iterable)) {
    return iterable[currentIndex];
  } else if (typeof iterable === "object" && iterable !== null) {
    const keys = Object.keys(iterable);
    const key = keys[currentIndex];
    if (key !== undefined) {
      return { key, value: (iterable as Record<string, unknown>)[key] };
    }
    return undefined;
  } else if (typeof iterable === "number") {
    return currentIndex;
  } else if (typeof iterable === "string") {
    return iterable[currentIndex];
  }

  return undefined;
}

/**
 * Set the loop variable within the scope of the loop.
 */
function setLoopVariable(thread: WorkflowExecution, variableName: string, value: unknown): void {
  // 循环作用域应该在 loopStartHandler 中通过 enterLoopScope() 创建
  const currentLoopScope = thread.variableScopes.loop[thread.variableScopes.loop.length - 1];
  if (currentLoopScope) {
    currentLoopScope[variableName] = value;
  }
}

/**
 * Update loop status
 */
function updateLoopState(loopState: LoopState): void {
  loopState.iterationCount++;
  loopState.currentIndex++;
}

/**
 * LoopStart node processing function
 * @param thread: WorkflowExecution instance
 * @param node: Node definition
 * @param context: Processor context (optional)
 * @returns: Execution result
 */
export async function loopStartHandler(
  executionEntity: WorkflowExecutionEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  const thread = executionEntity.getExecution();

  // Check if it is possible to execute.
  if (!canExecute(executionEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: thread.nodeResults.length + 1,
      executionTime: 0,
    };
  }

  const config = node.config as LoopStartNodeConfig;

  // Get or initialize the loop state.
  let loopState = getLoopState(thread);

  if (!loopState) {
    // On the first execution, the loop state is parsed and initialized.
    let resolvedIterable: unknown = null;
    let variableName: string | null = null;

    // If a dataSource is provided, then the iterable and variableName are parsed.
    if (config.dataSource) {
      // Parse an iterable (which can be a direct value or a variable expression).
      resolvedIterable = resolveIterable(config.dataSource.iterable, thread);
      variableName = config.dataSource.variableName;
    }

    // Create a loop state that includes the parsed iterable.
    loopState = {
      loopId: config.loopId,
      iterable: resolvedIterable,
      currentIndex: 0,
      maxIterations: config.maxIterations,
      iterationCount: 0,
      variableName: variableName,
    };

    setLoopState(thread, loopState);

    // Enter the new loop scope.
    if (!thread.variableScopes) {
      thread.variableScopes = {
        global: {},
        workflowExecution: {},
        local: [],
        loop: [],
      };
    }

    // Create a new loop scope and initialize the variables within that scope.
    const newLoopScope: Record<string, unknown> = {};
    for (const variable of thread.variables) {
      if (variable.scope === "loop") {
        newLoopScope[variable.name] = variable.value;
      }
    }
    thread.variableScopes.loop.push(newLoopScope);
  }

  // Check the loop conditions.
  const shouldContinue = checkLoopCondition(loopState);

  if (!shouldContinue) {
    // Loop ended, clearing loop state.
    clearLoopState(thread);

    // Leave the loop scope
    if (thread.variableScopes && thread.variableScopes.loop.length > 0) {
      thread.variableScopes.loop.pop();
    }

    return {
      loopId: config.loopId,
      shouldContinue: false,
      iterationCount: loopState.iterationCount,
      message: "Loop completed",
    };
  }

  // Get the current iteration value
  const currentValue = getCurrentValue(loopState);

  // Set loop variables to loop scope (only when data-driven loops)
  // Note: If dataSource is provided, variableName is required
  if (loopState.variableName !== null) {
    setLoopVariable(thread, loopState.variableName, currentValue);
  }

  // Update the loop status
  updateLoopState(loopState);

  // Save the updated loop state to the scope.
  setLoopState(thread, loopState);

  // Record execution history
  thread.nodeResults.push({
    step: thread.nodeResults.length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  // Return the execution results
  return {
    loopId: config.loopId,
    variableName: loopState.variableName,
    currentValue,
    iterationCount: loopState.iterationCount,
    shouldContinue: true,
  };
}
