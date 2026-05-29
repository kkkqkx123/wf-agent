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
import {
  ScriptInteractionCoordinator,
  type InputProvider,
} from "../../coordinators/script-interaction-coordinator.js";

/**
 * Context for INTERACTIVE_SCRIPT node handler
 * Provided by NodeHandlerContextFactory when the application has configured an input provider
 */
export interface InteractiveScriptHandlerContext {
  /** Input provider for user interaction in blocking/hybrid modes */
  inputProvider?: InputProvider;
  /** Abort signal for cancellation support */
  abortSignal?: AbortSignal;
}

/**
 * Interactive script node handler
 * @param globalContext Global application context
 * @param workflowExecutionEntity Workflow execution entity
 * @param node Runtime node
 * @param context Handler context (optional, contains inputProvider and abortSignal)
 * @returns Execution result
 */
export async function interactiveScriptHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  context?: InteractiveScriptHandlerContext,
): Promise<unknown> {
  const config = node.config as InteractiveScriptNodeConfig;

  try {
    const coordinator = new ScriptInteractionCoordinator(
      globalContext,
      workflowExecutionEntity,
      context?.inputProvider,
    );

    const result = await coordinator.executeWithInteraction(
      config.scriptName,
      config,
      context?.abortSignal,
    );

    if (!result.success) {
      throw new Error(result.error || "Interactive script execution failed");
    }

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
