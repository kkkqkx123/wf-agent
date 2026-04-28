/**
 * Script Node Processing Function
 * Responsible for executing SCRIPT nodes and running script code, supporting multiple scripting languages.
 *
 * Design Principles:
 * - Provides only pure execution capabilities; does not include business decision-making logic.
 * - All validation, security checks, and status determinations are the responsibility of the application layer.
 * - Execution history is recorded for use by higher-level systems.
 */

import type { Node, ScriptNodeConfig } from "@wf-agent/types";
import type { ThreadEntity } from "../../../entities/thread-entity.js";
import { now, getErrorMessage } from "@wf-agent/common-utils";
import { getContainer } from "../../../../core/di/container-config.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import type { ScriptRegistry } from "../../../../core/registry/script-registry.js";

/**
 * Script Node Processing Function
 * @param threadEntity ThreadEntity instance
 * @param node Node definition
 * @param context Processor context (optional)
 * @returns Execution result
 *
 * Note:
 * - The application layer is responsible for checking the Thread status (RUNNING/PAUSED/COMPLETED).
 * - The application layer is responsible for implementing risk level strategies (through middleware or interceptors).
 * - The application layer is responsible for script security verification (whitelist, sandbox configuration, etc.).
 */
export async function scriptHandler(
  threadEntity: ThreadEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  const config = node.config as ScriptNodeConfig;

  try {
    // Use the script service to execute the script.
    const container = getContainer();
    const scriptService = container.get(Identifiers.ScriptRegistry) as ScriptRegistry;
    const result = await scriptService.execute(config.scriptName);

    // Check the execution results.
    if (result.isErr()) {
      throw result.error;
    }

    // Record execution history
    threadEntity.addNodeResult({
      step: threadEntity.getNodeResults().length + 1,
      nodeId: node.id,
      nodeType: node.type,
      status: "COMPLETED",
      timestamp: now(),
    });

    // Return the execution result
    return result.value;
  } catch (error) {
    // Record the history of execution failures.
    threadEntity.addNodeResult({
      step: threadEntity.getNodeResults().length + 1,
      nodeId: node.id,
      nodeType: node.type,
      status: "FAILED",
      timestamp: now(),
      error: getErrorMessage(error),
    });

    throw error;
  }
}
