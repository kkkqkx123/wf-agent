/**
 * ResumeAgentLoopCommand - Resume Agent Loop Command
 *
 * Category: Management
 * Resumes a paused agent loop execution
 */

import {
  ManagementCommand,
  type CommandMetadataDefinition,
} from "../../shared/types/command.js";
import { validateAgentLoopControlParams } from "../../shared/operations/validators/agent-validators.js";
import type { CommandValidationResult } from "../../shared/types/command.js";
import type { ID } from "@wf-agent/types";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import { ExecutionError } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

/**
 * Resume Agent Loop command parameters
 */
export interface ResumeAgentLoopParams {
  /** Agent Loop ID to resume */
  agentLoopId: ID;
}

/**
 * Resume Agent Loop Command
 */
export class ResumeAgentLoopCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: ResumeAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "ResumeAgentLoopCommand",
      description: "Resume a paused agent loop",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      supportUndo: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<void> {
    const logger = createContextualLogger({
      component: "ResumeAgentLoopCommand",
      commandName: "ResumeAgentLoopCommand",
      agentLoopId: this.params.agentLoopId,
    });

    const startTime = Date.now();
    logger.info("Command execution started", {
      agentLoopId: this.params.agentLoopId,
    });

    try {
      const registry = this.dependencies.getAgentLoopRegistry();

      // Obtain the Agent Loop entity
      const entity = await registry.get(this.params.agentLoopId);
      if (!entity) {
        throw new ExecutionError(`Agent Loop not found: ${this.params.agentLoopId}`);
      }

      // Check if it is possible to restore.
      if (!entity.isPaused()) {
        throw new ExecutionError(`Agent Loop is not paused, cannot resume`);
      }

      // Perform the recovery operation.
      entity.resume();

      const duration = Date.now() - startTime;
      logger.info("Command execution completed successfully", undefined, {
        duration,
      });
    } catch (error) {
      const duration = Date.now() - startTime;
      logger.error("Command execution failed", undefined, { duration }, error as Error);
      throw error;
    }
  }

  validate(): CommandValidationResult {
    return validateAgentLoopControlParams(this.params.agentLoopId);
  }
}
