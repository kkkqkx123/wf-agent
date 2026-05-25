/**
 * Script Interaction Coordinator
 * Coordinates interactive script execution with user/LLM input handling
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { ScriptRegistry } from "../../../core/registry/script-registry.js";
import type { GlobalContext } from "../../../core/global-context.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ScriptInteractionCoordinator" });

/**
 * Script Interaction Coordinator
 * Handles interactive script execution by coordinating with the UserInteractionHandler
 */
export class ScriptInteractionCoordinator {
  private globalContext: GlobalContext;

  constructor(
    globalContext: GlobalContext,
    _workflowExecutionEntity: WorkflowExecutionEntity,
  ) {
    this.globalContext = globalContext;
  }

  /**
   * Execute a script with interaction support
   * @param scriptName Script name to execute
   * @returns Execution result
   */
  async executeWithInteraction(scriptName: string): Promise<unknown> {
    const scriptService = this.globalContext.container.get(Identifiers.ScriptRegistry) as ScriptRegistry;

    logger.debug("Starting interactive script execution", { scriptName });

    const result = await scriptService.execute(scriptName);

    if (result.isErr()) {
      throw result.error;
    }

    return result.value;
  }
}