/**
 * Route node processing function
 * Responsible for executing the ROUTE node, evaluating route conditions, and selecting the next node.
 */

import type { Node, RouteNodeConfig } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { Condition, EvaluationContext } from "@wf-agent/types";
import { ExecutionError } from "@wf-agent/types";
import { conditionEvaluator } from "@wf-agent/common-utils";
import { now, getErrorMessage } from "@wf-agent/common-utils";

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
function evaluateRouteCondition(condition: Condition, workflowExecutionEntity: WorkflowExecutionEntity): boolean {
  try {
    // Constructing the evaluation context
    const context: EvaluationContext = {
      variables: workflowExecutionEntity.getAllVariables(),
      input: workflowExecutionEntity.getInput(),
      output: workflowExecutionEntity.getOutput(),
    };

    return conditionEvaluator.evaluate(condition, context);
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
export async function routeHandler(workflowExecutionEntity: WorkflowExecutionEntity, node: Node): Promise<unknown> {
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

  // Evaluate routing conditions
  for (const route of sortedRoutes) {
    if (evaluateRouteCondition(route.condition, workflowExecutionEntity)) {
      return {
        status: "COMPLETED",
        selectedNode: route.targetNodeId,
      };
    }
  }

  // No matching routes found; use the default target.
  if (config.defaultTargetNodeId) {
    return {
      status: "COMPLETED",
      selectedNode: config.defaultTargetNodeId,
    };
  }

  throw new ExecutionError(
    "No route matched and no default target specified",
    node.id,
    workflowExecutionEntity.getWorkflowId(),
  );
}
