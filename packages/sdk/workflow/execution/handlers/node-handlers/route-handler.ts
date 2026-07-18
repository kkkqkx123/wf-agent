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
  const context: EvaluationContext = {
    variables: workflowExecutionEntity.getAllVariables(),
    input: workflowExecutionEntity.getInput(),
    output: workflowExecutionEntity.getOutput(),
  };

  try {
    return conditionEvaluator.evaluate(condition, context);
  } catch (error) {
    const type = condition.type;
    const errorMsg =
      type === "expression"
        ? `Failed to evaluate route condition: ${condition.expression}`
        : `Failed to evaluate route condition of type ${type}`;
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

  // Sort routing rules by priority (descending), with original index as tiebreaker for determinism
  const sortedRoutes = config.routes
    .map((route, index) => ({ route, index }))
    .sort((a, b) => {
      const priorityDiff = (b.route.priority || 0) - (a.route.priority || 0);
      return priorityDiff !== 0 ? priorityDiff : a.index - b.index;
    })
    .map(({ route }) => route);

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
