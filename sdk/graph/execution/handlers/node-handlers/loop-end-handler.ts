/**
 * LoopEnd node processing function
 * Responsible for executing the LOOP_END node, updating loop variables, and checking for interruption conditions.
 */

import type { Node, LoopEndNodeConfig } from "@wf-agent/types";
import type { Thread } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";
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
function canExecute(threadEntity: ThreadEntity, node: Node): boolean {
  if (threadEntity.getStatus() !== "RUNNING") {
    return false;
  }

  const thread = threadEntity.getThread();
  const config = node.config as LoopEndNodeConfig;
  const loopState = getLoopState(thread);

  // Check if the loop status exists.
  if (!loopState) {
    return false;
  }

  return true;
}

/**
 * Get the loop status
 */
function getLoopState(thread: Thread): LoopState | undefined {
  const currentLoopScope = thread.variableScopes.loop[thread.variableScopes.loop.length - 1] as
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
function clearLoopState(thread: Thread): void {
  // Clear the loop state object
  const currentLoopScope = thread.variableScopes.loop[thread.variableScopes.loop.length - 1];
  if (currentLoopScope) {
    delete currentLoopScope[`__loop_state`];
  }

  // Leave the loop scope.
  if (thread.variableScopes && thread.variableScopes.loop.length > 0) {
    thread.variableScopes.loop.pop();
  }
}

/**
 * Evaluating interrupt conditions
 */
function evaluateBreakCondition(breakCondition: Condition, thread: Thread): boolean {
  try {
    // Constructing the evaluation context
    const context: EvaluationContext = {
      variables: thread.variableScopes.thread || {},
      input: thread.input || {},
      output: thread.output || {},
    };

    // Use ConditionEvaluator to evaluate conditions.
    return conditionEvaluator.evaluate(breakCondition, context);
  } catch (error) {
    throw new ExecutionError(
      `Failed to evaluate break condition: ${getErrorMessage(error)}`,
      thread.currentNodeId,
      thread.workflowId,
      {
        breakCondition,
        variables: thread.variableScopes.thread,
        input: thread.input,
        output: thread.output,
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
 * @param thread Thread instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function loopEndHandler(
  threadEntity: ThreadEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  const thread = threadEntity.getThread();

  // Check if it is possible to execute.
  if (!canExecute(threadEntity, node)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: thread.nodeResults.length + 1,
      executionTime: 0,
    };
  }

  const config = node.config as LoopEndNodeConfig;

  // Get the loop status
  const loopState = getLoopState(thread);

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
    shouldBreak = evaluateBreakCondition(config.breakCondition, thread);
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
    clearLoopState(thread);
  }

  // Record execution history
  thread.nodeResults.push({
    step: thread.nodeResults.length + 1,
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
