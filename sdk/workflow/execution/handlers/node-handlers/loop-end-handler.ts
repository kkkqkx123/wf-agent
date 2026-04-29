/**
 * LoopEnd node processing function
 * Responsible for executing the LOOP_END node, updating loop variables, and checking for interruption conditions.
 */

import type { Node, LoopEndNodeConfig } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { Condition, EvaluationContext } from "@wf-agent/types";
import { ExecutionError, NotFoundError } from "@wf-agent/types";
import { conditionEvaluator } from "@wf-agent/common-utils";
import { now, getErrorMessage, getErrorOrUndefined } from "@wf-agent/common-utils";

/**
 * Loop state
 */
interface LoopState {
  loopId: string;
  iterable: unknown | null; // Can be null (during counting loops).
  currentIndex: number;
  maxIterations: number;
  iterationCount: number;
  variableName: string | null; // Can be null (during counting loops).
}

/**
 * Check whether the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity, node: Node): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }

  const workflowExecution = workflowExecutionEntity.getExecution();
  const config = node.config as LoopEndNodeConfig;
  const loopState = getLoopState(workflowExecution);

  // Check if the loop status exists.
  if (!loopState) {
    return false;
  }

  return true;
}

/**
 * Get the loop status
 */
function getLoopState(workflowExecution: WorkflowExecution): LoopState | undefined {
  const currentLoopScope = workflowExecution.variableScopes.loop[workflowExecution.variableScopes.loop.length - 1] as
    | Record<string, unknown>
    | undefined;
  if (currentLoopScope) {
    return currentLoopScope[`__loop_state`] as LoopState | undefined;
  }
  return undefined;
}

/**
 * Clear loop state and scope.
 */
function clearLoopState(workflowExecution: WorkflowExecution): void {
  // Clear the loop state object
  const currentLoopScope = workflowExecution.variableScopes.loop[workflowExecution.variableScopes.loop.length - 1];
  if (currentLoopScope) {
    delete currentLoopScope[`__loop_state`];
  }

  // Leave the loop scope.
  if (workflowExecution.variableScopes && workflowExecution.variableScopes.loop.length > 0) {
    workflowExecution.variableScopes.loop.pop();
  }
}

/**
 * Evaluating interrupt conditions
 */
function evaluateBreakCondition(breakCondition: Condition, workflowExecution: WorkflowExecution): boolean {
  try {
    // Constructing the evaluation context
    const context: EvaluationContext = {
      variables: workflowExecution.variableScopes.thread || {},
      input: workflowExecution.input || {},
      output: workflowExecution.output || {},
    };

    // Use ConditionEvaluator to evaluate conditions.
    return conditionEvaluator.evaluate(breakCondition, context);
  } catch (error) {
    throw new ExecutionError(
      `Failed to evaluate break condition: ${getErrorMessage(error)}`,
      workflowExecution.currentNodeId,
      workflowExecution.workflowId,
      {
        breakCondition,
        variables: workflowExecution.variableScopes.thread,
        input: workflowExecution.input,
        output: workflowExecution.output,
      },
      getErrorOrUndefined(error),
    );
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
 * Update the loop status
 */
function updateLoopState(loopState: LoopState): void {
  loopState.iterationCount++;
  loopState.currentIndex++;
}

/**
 * LoopEnd node processing function
 * @param workflowExecutionEntity WorkflowExecutionEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function loopEndHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  const workflowExecution = workflowExecutionEntity.getExecution();

  // Check if it is possible to execute.
  if (!canExecute(workflowExecutionEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: workflowExecution.nodeResults.length + 1,
      executionTime: 0,
    };
  }

  const config = node.config as LoopEndNodeConfig;

  // Get the loop status
  const loopState = getLoopState(workflowExecution);

  if (!loopState) {
    throw new NotFoundError(
      `Loop state not found for loopId: ${config.loopId}`,
      "loopState",
      config.loopId,
      {
        nodeId: node.id,
        loopId: config.loopId,
      },
    );
  }

  // Evaluating interrupt conditions
  let shouldBreak = false;
  if (config.breakCondition) {
    shouldBreak = evaluateBreakCondition(config.breakCondition, workflowExecution);
  }

  // Check the loop condition.
  const loopConditionMet = checkLoopCondition(loopState);

  // Decide whether to continue the loop.
  const shouldContinue = !shouldBreak && loopConditionMet;

  // If you need to continue the loop, update the loop status.
  if (shouldContinue) {
    updateLoopState(loopState);
  } else {
    // Loop ended, clearing the loop state and scope.
    clearLoopState(workflowExecution);
  }

  // Record execution history
  workflowExecution.nodeResults.push({
    step: workflowExecution.nodeResults.length + 1,
    nodeId: node.id,
    nodeType: node.type,
    status: "COMPLETED",
    timestamp: now(),
  });

  // Return the execution results.
  return {
    loopId: config.loopId,
    shouldContinue,
    shouldBreak,
    loopConditionMet,
    iterationCount: loopState.iterationCount,
    nextNodeId: shouldContinue ? config.loopStartNodeId : undefined,
  };
}
