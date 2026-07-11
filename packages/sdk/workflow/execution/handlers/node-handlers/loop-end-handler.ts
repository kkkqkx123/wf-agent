/**
 * LoopEnd node processing function
 * Responsible for executing the LOOP_END node, updating loop variables, and checking for interruption conditions.
 */

import type { RuntimeNode, LoopEndNodeConfig } from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { Condition, EvaluationContext } from "@wf-agent/types";
import { ExecutionError, NotFoundError } from "@wf-agent/types";
import { conditionEvaluator } from "../../../../services/evaluation/index.js";
import { now, getErrorMessage, getErrorOrUndefined } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "loop-end-handler" });

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
  /** Number of consecutive failures in this loop */
  consecutiveFailures: number;
  /** Total number of failures across all iterations */
  totalFailures: number;
}

/**
 * Check whether the node can be executed (idempotency check — status check is handled centrally).
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity, node: RuntimeNode): boolean {
  if (workflowExecutionEntity.getNodeResults().some(result => result.nodeId === node.id)) {
    return false;
  }

  const loopState = getLoopState(workflowExecutionEntity);

  // Check if the loop status exists.
  if (!loopState) {
    return false;
  }

  return true;
}

/**
 * Get the loop state from VariableManager's scope stack
 */
function getLoopState(executionEntity: WorkflowExecutionEntity): LoopState | undefined {
  const manager = executionEntity.variableStateManager;
  return manager.getVariable("__loop_state") as LoopState | undefined;
}

/**
 * Clear loop state using VariableManager
 */
function clearLoopState(executionEntity: WorkflowExecutionEntity): void {
  const manager = executionEntity.variableStateManager;
  // Remove loop state
  manager.deleteVariable("__loop_state");
  // Phase 2: Scope exit via exitSubgraphScope() will be added here
  // when VariableManager supports scope stack. Variable export is handled
  // through exportVariables API for now.
}

/**
 * Evaluating interrupt conditions
 */
function evaluateBreakCondition(
  breakCondition: Condition,
  workflowExecution: WorkflowExecution,
  executionEntity?: WorkflowExecutionEntity,
  loopId?: string,
): boolean {
  try {
    // Constructing the evaluation context
    const variables = executionEntity?.variableStateManager.getAllVariables() || {};
    const context: EvaluationContext = {
      variables,
      input: workflowExecution.input || {},
      output: workflowExecution.output || {},
    };

    // Handle discriminated union Condition type
    const conditionRecord = (breakCondition as unknown) as Record<string, unknown>;
    const conditionType = (conditionRecord['type'] as string) ?? "expression";

    // For expression conditions, use the cached evaluator for backward compatibility
    if (conditionType === "expression" && executionEntity) {
      const depManager = executionEntity.getDepManager();
      const key = `loopBreak:${loopId || ""}`;
      const expression = conditionRecord['expression'] as string;
      const cached = depManager.getTrackedExpression(key);
      if (cached) {
        return Boolean(depManager.evaluateIfChanged(key, context));
      }
      depManager.register(key, expression, context);
      return Boolean(depManager.getTrackedExpression(key)?.lastResult);
    }

    // For other condition types or no executionEntity, use unified evaluator
    return conditionEvaluator.evaluate(breakCondition, context);
  } catch (error) {
    throw new ExecutionError(
      `Failed to evaluate break condition: ${getErrorMessage(error)}`,
      workflowExecution.currentNodeId,
      workflowExecution.workflowId,
      {
        breakCondition,
        variables: executionEntity?.variableStateManager.getAllVariables(),
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
  node: RuntimeNode,
): Promise<unknown> {
  const workflowExecution = workflowExecutionEntity.getWorkflowExecutionData();

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
  const loopState = getLoopState(workflowExecutionEntity);

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

  // Track iteration failures: scan nodeResults for FAILED nodes in this iteration
  // (nodes that executed after the last LOOP_START or LOOP_END)
  const iterationFailed = workflowExecutionEntity
    .getNodeResults()
    .some(
      r =>
        (r.status === "FAILED" || r.status === "CANCELLED") &&
        r.nodeId !== node.id,
    );

  if (iterationFailed) {
    loopState.consecutiveFailures = (loopState.consecutiveFailures || 0) + 1;
    loopState.totalFailures = (loopState.totalFailures || 0) + 1;
    logger.debug("Loop iteration detected failure", {
      loopId: config.loopId,
      consecutiveFailures: loopState.consecutiveFailures,
      totalFailures: loopState.totalFailures,
      iterationCount: loopState.iterationCount,
    });
  } else {
    // Reset consecutive failures on successful iteration
    loopState.consecutiveFailures = 0;
  }

  // Save updated loop state back to VariableManager
  const manager = workflowExecutionEntity.variableStateManager;
  manager.setVariable("__loop_state", loopState);

  // Evaluating interrupt conditions
  let shouldBreak = false;
  if (config.breakCondition) {
    shouldBreak = evaluateBreakCondition(
      config.breakCondition,
      workflowExecution,
      workflowExecutionEntity,
      config.loopId,
    );
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
    clearLoopState(workflowExecutionEntity);
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
    breakTriggered: shouldBreak,
    iterationCount: loopState.iterationCount,
    nextIteration: shouldContinue,
  };
}
