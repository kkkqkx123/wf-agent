/**
 * Script Node Processing Function
 * Responsible for executing SCRIPT nodes and running script code, supporting multiple scripting languages.
 *
 * Design Principles:
 * - Provides only pure execution capabilities; does not include business decision-making logic.
 * - All validation, security checks, and status determinations are the responsibility of the application layer.
 * - Execution history is recorded for use by higher-level systems.
 *
 * Supports:
 * - Standard scriptName execution via ScriptRegistry
 * - Inline template execution with variable substitution
 * - Flow execution via flowId reference
 * - Executor mode selection (direct/shared)
 */

import type { RuntimeNode, ScriptNodeConfig } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import { now, getErrorMessage } from "@wf-agent/common-utils";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import type { ScriptRegistry } from "../../../../core/registry/script-registry.js";
import type { GlobalContext } from "../../../../core/global-context.js";
import { getSkippedResult } from "./can-execute.js";

/**
 * Script Node Processing Function
 * @param globalContext Global context for accessing DI container
 * @param workflowExecutionEntity WorkflowExecutionEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 *
 * Note:
 * - The application layer is responsible for checking the WorkflowExecution status (RUNNING/PAUSED/COMPLETED).
 * - The application layer is responsible for implementing risk level strategies (through middleware or interceptors).
 * - The application layer is responsible for script security verification (whitelist, sandbox configuration, etc.).
 */
export async function scriptHandler(
  globalContext: GlobalContext,
  workflowExecutionEntity: WorkflowExecutionEntity,
  node: RuntimeNode,
  _context?: unknown,
): Promise<unknown> {
  const skipped = getSkippedResult(workflowExecutionEntity, node);
  if (skipped) return skipped;

  const config = node.config as ScriptNodeConfig;

  try {
    const scriptService = globalContext.container.get(Identifiers.ScriptRegistry) as ScriptRegistry;

    let result;

    if (config.flowId) {
      result = await scriptService.executeFlow(config.flowId);
    } else if (config.template) {
      const { ScriptEngine } = await import("../../../../core/script/engine/script-engine.js");
      const engine = new ScriptEngine();
      const script = {
        id: node.id,
        name: config.scriptName || "inline",
        description: "Inline script from node config",
        template: config.template,
        executor: config.executor,
        options: { timeout: 30000 },
      };
      result = await engine.execute(script);
    } else {
      result = await scriptService.execute(config.scriptName);
    }

    if (
      result &&
      typeof result === "object" &&
      "isErr" in result &&
      typeof result.isErr === "function"
    ) {
      if (result.isErr()) {
        throw result.error;
      }
      result = result.value;
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
