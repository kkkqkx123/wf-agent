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
 * Generate a cache key for a route condition
 */
function generateRouteCacheKey(condition: Record<string, unknown>): string {
  const type = (condition['type'] as string) ?? "expression";
  switch (type) {
    case "expression":
      return `route:expr:${condition['expression'] as string}`;
    case "predicate":
      return `route:pred:${condition['predicateType'] as string}:${condition['variable'] as string}`;
    case "schema":
      return `route:schema:${condition['variable'] as string}`;
    case "script":
      // Script type is not cached, return empty string
      return "";
    default:
      return `route:${type}:${JSON.stringify(condition)}`;
  }
}

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

    // Use unified conditionEvaluator with cache key for all condition types
    const conditionRecord = (condition as unknown) as Record<string, unknown>;
    const cacheKey = generateRouteCacheKey(conditionRecord);
    return conditionEvaluator.evaluate(condition, context, cacheKey || undefined);
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
