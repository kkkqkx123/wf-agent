/**
 * Route node processing function
 * Responsible for executing the ROUTE node, evaluating route conditions, and selecting the next node.
 */

import type { RuntimeNode, RouteNodeConfig } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { Condition, EvaluationContext } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";

/**
 * Check if the node can be executed.
 */
function canExecute(workflowExecutionEntity: WorkflowExecutionEntity): boolean {
  if (workflowExecutionEntity.getStatus() !== "RUNNING") {
    return false;
  }
  return true;
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

    // Use DependencyManager for per-execution caching of compiled conditions.
    // The key `route:${nodeId}:${targetNodeId}` persists across handler calls
    // within the same execution, avoiding repeated AST parsing.
    const depManager = workflowExecutionEntity.getDepManager();
    const expression = condition.expression;
    const cached = depManager.getTrackedExpression(expression);
    if (cached) {
      return Boolean(depManager.evaluateIfChanged(expression, context));
    }
    depManager.register(expression, expression, context);
    return Boolean(depManager.getTrackedExpression(expression)?.lastResult);
  } catch (error) {
    throw new ExecutionError(
      `Failed to evaluate route condition: ${condition.expression}`,
      workflowExecutionEntity.getCurrentNodeId(),
      workflowExecutionEntity.getWorkflowId(),
      { expression: condition.expression, originalError: error },
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
export async function routeHandler(workflowExecutionEntity: WorkflowExecutionEntity, node: RuntimeNode): Promise<unknown> {
  // Check if it can be executed.
  if (!canExecute(workflowExecutionEntity)) {
    return {
      nodeId: node.id,
      nodeType: node.type,
      status: "SKIPPED",
      step: workflowExecutionEntity.getNodeResults().length + 1,
      executionTime: 0,
    };
  }

  const config = node.config as RouteNodeConfig;

  // Sort routing rules by priority.
  const sortedRoutes = [...config.routes].sort((a, b) => (b.priority || 0) - (a.priority || 0));

  // Evaluate all routing conditions for the output record
  const evaluatedConditions: Array<{ condition: string; result: boolean; targetNodeId: string }> = [];
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
