/**
 * Route node processing function
 * Responsible for executing the ROUTE node, evaluating route conditions, and selecting the next node.
 */

import type { RuntimeNode, RouteNodeConfig } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { Condition, EvaluationContext } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { conditionEvaluator } from "../../../../services/evaluation/index.js";

/**
 * Evaluating routing conditions
 */
function evaluateRouteCondition(
  condition: Condition,
  workflowExecutionEntity: WorkflowExecutionEntity,
): boolean {
  try {
    // Constructing the evaluation context
    const context: EvaluationContext = {
      variables: workflowExecutionEntity.getAllVariables(),
      input: workflowExecutionEntity.getInput(),
      output: workflowExecutionEntity.getOutput(),
    };

    // Handle discriminated union Condition type
    const conditionRecord = (condition as unknown) as Record<string, unknown>;
    const conditionType = (conditionRecord['type'] as string) ?? "expression";

    // For expression conditions, use the cached evaluator for backward compatibility
    if (conditionType === "expression") {
      const depManager = workflowExecutionEntity.getDepManager();
      const expression = conditionRecord['expression'] as string;
      const cached = depManager.getTrackedExpression(expression);
      if (cached) {
        return Boolean(depManager.evaluateIfChanged(expression, context));
      }
      depManager.register(expression, expression, context);
      return Boolean(depManager.getTrackedExpression(expression)?.lastResult);
    } else {
      // For other condition types, use the unified conditionEvaluator
      return conditionEvaluator.evaluate(condition, context);
    }
  } catch (error) {
    const conditionRecord = (condition as unknown) as Record<string, unknown>;
    const errorMsg =
      ((conditionRecord['type'] as string) === "expression") || !conditionRecord['type']
        ? `Failed to evaluate route condition: ${conditionRecord['expression']}`
        : `Failed to evaluate route condition of type ${conditionRecord['type']}`;
    throw new ExecutionError(
      errorMsg,
      workflowExecutionEntity.getCurrentNodeId(),
      workflowExecutionEntity.getWorkflowId(),
      { condition, originalError: error },
    );
  }
}

/**
 * Route node processing function
 * @param workflowExecutionEntity WorkflowExecutionEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function routeHandler(
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
): Promise<unknown> {
  const config = node.config as RouteNodeConfig;

  // Sort routing rules by priority.
  const sortedRoutes = [...config.routes].sort((a, b) => (b.priority || 0) - (a.priority || 0));

  // Evaluate all routing conditions for the output record
  const evaluatedConditions: Array<{ condition: string; result: boolean; targetNodeId: string }> =
    [];
  for (const route of sortedRoutes) {
    const result = evaluateRouteCondition(route.condition, workflowExecutionEntity);
    evaluatedConditions.push({
      condition: JSON.stringify(route.condition),
      result,
      targetNodeId: route.targetNodeId,
    });
    if (result) {
      return {
        selectedRoute: route.targetNodeId,
        evaluatedConditions,
      };
    }
  }

  // No matching routes found; use the default target.
  if (config.defaultTargetNodeId) {
    return {
      selectedRoute: config.defaultTargetNodeId,
      evaluatedConditions,
    };
  }

  throw new ExecutionError(
    "No route matched and no default target specified",
    node.id,
    workflowExecutionEntity.getWorkflowId(),
  );
}
