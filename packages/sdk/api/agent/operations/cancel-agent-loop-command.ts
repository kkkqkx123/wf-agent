/**
 * CancelAgentLoopCommand - Cancel Agent Loop Command
 *
 * Category: Management
 * Cancels a running or paused agent loop execution
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
 * Cancel Agent Loop command parameters
 */
export interface CancelAgentLoopParams {
  /** Agent Loop ID to cancel */
  agentLoopId: ID;
  /** Optional reason for cancellation */
  reason?: string;
}

/**
 * Cancel Agent Loop Command
 */
export class CancelAgentLoopCommand extends ManagementCommand<void> {
  constructor(
    private readonly params: CancelAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "CancelAgentLoopCommand",
      description: "Cancel a running or paused agent loop",
      category: "management",
      requiresAuth: false,
      version: "1.0.0",
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<void> {
    const logger = createContextualLogger({
      component: "CancelAgentLoopCommand",
      commandName: "CancelAgentLoopCommand",
      agentLoopId: this.params.agentLoopId,
    });

    const startTime = Date.now();
    logger.info("Command execution started", {
      agentLoopId: this.params.agentLoopId,
      reason: this.params.reason,
    });

    try {
      const registry = this.dependencies.getAgentLoopRegistry();

      // Obtain the Agent Loop entity
      const entity = await registry.get(this.params.agentLoopId);
      if (!entity) {
        throw new ExecutionError(`Agent Loop not found: ${this.params.agentLoopId}`);
      }

      // Check if it's possible to cancel.
      if (!entity.isRunning() && !entity.isPaused()) {
        throw new ExecutionError(`Agent Loop is not running or paused, cannot cancel`);
      }

      // Execute the cancel operation.
      entity.stop();

      const duration = Date.now() - startTime;
      logger.info("Command execution completed successfully", undefined, {
        duration,
        reason: this.params.reason,
      });
    } catch (error) {
      const duration = Date.now() - startTime;
      logger.error(
        "Command execution failed",
        undefined,
        { duration, reason: this.params.reason },
        error as Error
      );
      throw error;
    }
  }

  validate(): CommandValidationResult {
    return validateAgentLoopControlParams(this.params.agentLoopId);
  }
}
