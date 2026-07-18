/**
 * LoopStart node processing function
 * Responsible for executing the LOOP_START node, initializing loop variables, and setting loop conditions.
 */

import type { RuntimeNode, LoopStartNodeConfig } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { ExecutionError, ValidationError, RuntimeValidationError } from "@wf-agent/types";
import { getErrorMessage } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "loop-start-handler" });

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
  /** Number of consecutive failures in this loop (for onIterationFailure strategy) */
  consecutiveFailures: number;
  /** Total number of failures across all iterations */
  totalFailures: number;
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
 * - Variable expressions: {{input.list}}, {{execution.items}}, {{global.data}}
 */
function resolveIterable(
  iterableConfig: unknown,
  workflowExecution: WorkflowExecution,
  executionEntity?: WorkflowExecutionEntity,
): unknown {
  // If no iterable configuration is provided, return null (counting loop mode).
  if (iterableConfig === undefined || iterableConfig === null) {
    return null;
  }

  // If it is a string, check whether it is a variable expression.
  if (typeof iterableConfig === "string") {
    const varExprPattern = /^\{\{([\w.]+)\}\}$/;
    const match = iterableConfig.match(varExprPattern);

    if (match && match[1]) {
      // This is a variable expression that needs to be parsed from workflowExecution.
      const varPath = match[1];
      const parts = varPath.split(".");
      const scope = parts[0];

      try {
        let value: unknown;

        // Get variables based on their scope.
        switch (scope) {
          case "input":
            value = workflowExecution.input;
            // Parse nested paths
            for (let i = 1; i < parts.length; i++) {
              value = (value as Record<string, unknown>)?.[parts[i]!];
            }
            break;

          case "output":
            value = workflowExecution.output;
            for (let i = 1; i < parts.length; i++) {
              value = (value as Record<string, unknown>)?.[parts[i]!];
            }
            break;

          case "execution":
            // Use VariableManager API to get execution variable
            if (executionEntity?.variableStateManager) {
              value = executionEntity.variableStateManager.getAllVariables();
            } else {
              value = {};
            }
            for (let i = 1; i < parts.length; i++) {
              value = (value as Record<string, unknown>)?.[parts[i]!];
            }
            break;

          default:
            throw new RuntimeValidationError(
              `Invalid variable scope '${scope}'. Supported scopes: input, output, execution`,
              {
                operation: "handle",
                field: "loop.scope",
                value: scope,
              },
            );
        }

        if (value === undefined) {
          throw new ExecutionError(
            `Variable '${varPath}' not found in workflow execution context`,
            workflowExecution.currentNodeId,
            workflowExecution.workflowId,
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
          workflowExecution.currentNodeId,
          workflowExecution.workflowId,
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
 * Get the loop state from VariableManager's scope stack
 */
function getLoopState(executionEntity: WorkflowExecutionEntity): LoopState | undefined {
  const manager = executionEntity.variableStateManager;
  // Loop state is stored with special key in current scope
  return manager.getVariable("__loop_state") as LoopState | undefined;
}

/**
 * Set the loop state within the current scope using VariableManager
 */
function setLoopState(executionEntity: WorkflowExecutionEntity, loopState: LoopState): void {
  const manager = executionEntity.variableStateManager;
  manager.setVariable("__loop_state", loopState);
}

/**
 * Clear the loop state (only delete the state object; scope is cleared on exit)
 */
function clearLoopState(executionEntity: WorkflowExecutionEntity): void {
  const manager = executionEntity.variableStateManager;
  // Just remove the loop state, the scope itself will be popped on exit
  manager.deleteVariable("__loop_state");
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
 * Set the loop variable within the current scope using VariableManager
 */
function setLoopVariable(
  executionEntity: WorkflowExecutionEntity,
  variableName: string,
  value: unknown,
): void {
  const manager = executionEntity.variableStateManager;
  manager.setVariable(variableName, value);
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
 * @param workflowExecution: WorkflowExecution instance
 * @param node: RuntimeNode definition
 * @param context: Processor context (optional)
 * @returns: Execution result
 */
export async function loopStartHandler(
  executionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
): Promise<unknown> {
  const workflowExecution = executionEntity.getWorkflowExecutionData();

  const config = node.config as LoopStartNodeConfig;

  // NEW: Handle explicit variable input mapping for loop
  await handleLoopVariableInputs(executionEntity, config);

  // Get or initialize the loop state.
  let loopState = getLoopState(executionEntity);

  if (!loopState) {
    // On the first execution, the loop state is parsed and initialized.
    let resolvedIterable: unknown = null;
    let variableName: string | null = null;

    // If a dataSource is provided, then the iterable and variableName are parsed.
    if (config.dataSource) {
      // Parse an iterable (which can be a direct value or a variable expression).
      resolvedIterable = resolveIterable(
        config.dataSource.iterable,
        workflowExecution,
        executionEntity,
      );
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
      consecutiveFailures: 0,
      totalFailures: 0,
    };

    // Phase 2: Scope isolation via enterSubgraphScope() will be added here
    // when VariableManager supports scope stack. For now, explicit variable
    // import via handleLoopVariableInputs (above) handles variable isolation
    // through deep-cloning.

    setLoopState(executionEntity, loopState);
  }

  // Check iteration failure strategy (only applies when loopState already existed, i.e., not first iteration)
  // Detect failures in the previous iteration by scanning nodeResults for FAILED nodes
  // that belong to the loop body (between the last LOOP_START/LOOP_END and now).
  const onIterationFailure = config.onIterationFailure ?? "fail";
  const maxConsecutiveFailures = config.maxConsecutiveFailures ?? 0;

  if (loopState.totalFailures > 0) {
    // The previous iteration had failures — apply strategy
    if (onIterationFailure === "fail") {
      // Terminate the loop
      clearLoopState(executionEntity);
      return {
        loopId: config.loopId,
        iterationCount: loopState.iterationCount,
        maxIterations: loopState.maxIterations,
        hasMoreIterations: false,
      };
    }

    // For skip/continue: check max consecutive failures threshold
    if (
      maxConsecutiveFailures > 0 &&
      loopState.consecutiveFailures >= maxConsecutiveFailures
    ) {
      logger.warn("Loop terminated due to max consecutive failures", {
        loopId: config.loopId,
        consecutiveFailures: loopState.consecutiveFailures,
        maxConsecutiveFailures,
      });
      clearLoopState(executionEntity);
      return {
        loopId: config.loopId,
        iterationCount: loopState.iterationCount,
        maxIterations: loopState.maxIterations,
        hasMoreIterations: false,
      };
    }

    // Log the skip/continue decision
    logger.info("Loop iteration failure handled by strategy", {
      loopId: config.loopId,
      onIterationFailure,
      consecutiveFailures: loopState.consecutiveFailures,
      totalFailures: loopState.totalFailures,
      iterationCount: loopState.iterationCount,
    });
  }

  // Check the loop conditions.
  const shouldContinue = checkLoopCondition(loopState);

  if (!shouldContinue) {
    // Loop ended, clearing loop state.
    clearLoopState(executionEntity);

    // Phase 2: Scope exit via exitSubgraphScope() will be added here
    // when VariableManager supports scope stack. Variables are currently
    // isolated through deep-cloning in importVariables/exportVariables.

    return {
      loopId: config.loopId,
      iterationCount: loopState.iterationCount,
      maxIterations: loopState.maxIterations,
      hasMoreIterations: false,
    };
  }

  // Get the current iteration value
  const currentValue = getCurrentValue(loopState);

  // Set loop variables to loop scope (only when data-driven loops)
  // Note: If dataSource is provided, variableName is required
  if (loopState.variableName !== null) {
    setLoopVariable(executionEntity, loopState.variableName, currentValue);
  }

  // Update the loop status
  updateLoopState(loopState);

  // Save the updated loop state to the scope.
  setLoopState(executionEntity, loopState);

  // Return the execution results
  return {
    loopId: config.loopId,
    iterationCount: loopState.iterationCount,
    maxIterations: loopState.maxIterations,
    hasMoreIterations: loopState.iterationCount < loopState.maxIterations,
  };
}

/**
 * Handle explicit variable input mapping for loop
 * Maps parent workflow variables to loop internal variables using importVariables API
 *
 * @param executionEntity WorkflowExecution entity
 * @param config LoopStart node configuration
 */
async function handleLoopVariableInputs(
  executionEntity: WorkflowExecutionEntity,
  config: LoopStartNodeConfig,
): Promise<void> {
  const manager = executionEntity.variableStateManager;

  // Process explicit variable inputs using the new importVariables API
  if (config.variableInputs && config.variableInputs.length > 0) {
    // For loops, we import from the same manager (self-reference)
    // The importVariables method will deep clone the values
    manager.importVariables(manager, config.variableInputs);
  }
}
