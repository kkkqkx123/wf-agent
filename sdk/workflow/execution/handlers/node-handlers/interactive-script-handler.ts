/**
 * Interactive Script Node Processing Function
 * Responsible for executing INTERACTIVE_SCRIPT nodes that require user interaction
 *
 * Design Principles:
 * - Wraps standard script execution with interactive capabilities
 * - Delegates interaction coordination to ScriptInteractionCoordinator
 * - Records execution history for higher-level systems
 */

import type { RuntimeNode, InteractiveScriptNodeConfig } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { now, getErrorMessage } from "@wf-agent/common-utils";
import type { GlobalContext } from "../../../../core/global-context.js";
import { ScriptInteractionCoordinator } from "../../coordinators/script-interaction-coordinator.js";

/**
 * Interactive Script Node Processing Function
 * @param globalContext Global context for accessing DI container
 * @param workflowExecutionEntity WorkflowExecutionEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 */
export async function interactiveScriptHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  _context?: unknown,
): Promise<unknown> {
  const config = node.config as InteractiveScriptNodeConfig;

  try {
    const coordinator = new ScriptInteractionCoordinator(globalContext, workflowExecutionEntity);

    const result = await coordinator.executeWithInteraction(config.scriptName);

    workflowExecutionEntity.addNodeResult({
      step: workflowExecutionEntity.getNodeResults().length + 1,
      nodeId: node.id,
      nodeType: node.type,
      status: "COMPLETED",
      timestamp: now(),
    });

    return result;
  } catch (error) {
    workflowExecutionEntity.addNodeResult({
      step: workflowExecutionEntity.getNodeResults().length + 1,
      nodeId: node.id,
      nodeType: node.type,
      status: "FAILED",
      timestamp: now(),
      error: getErrorMessage(error),
    });

    throw error;
  }
}