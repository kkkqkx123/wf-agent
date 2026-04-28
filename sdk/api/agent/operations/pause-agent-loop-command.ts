/**
 * PauseAgentLoopCommand - Pause Agent Loop Command
 *
 * Responsibilities:
 * - Encapsulates Agent Loop pause operation as Command pattern
 * - Provides unified API layer interface
 * - Supports parameter validation
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for AgentLoopRegistry
 * - Parameter validation is completed in validate() method
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../shared/types/command.js";
import type { ID } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Pause Agent Loop Command Parameters
 */
export interface PauseAgentLoopParams {
  /** Agent Loop ID */
  agentLoopId: ID;
}

/**
 * Pause Agent Loop Command
 *
 * Workflow:
 * 1. Validate parameters (agentLoopId is required)
 * 2. Get AgentLoopEntity
 * 3. Call pause() method to pause execution
 * 4. Return pause result
 */
export class PauseAgentLoopCommand extends BaseCommand<void> {
  constructor(
    private readonly params: PauseAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<void> {
    const registry = this.dependencies.getAgentLoopRegistry();

    // Getting the Agent Loop Entity
    const entity = registry.get(this.params.agentLoopId);
    if (!entity) {
      throw new Error(`Agent Loop not found: ${this.params.agentLoopId}`);
    }

    // Check if you can pause
    if (!entity.isRunning()) {
      throw new Error(`Agent Loop is not running, cannot pause`);
    }

    // Perform a pause operation
    entity.pause();
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Validation: agentLoopId must be provided
    if (!this.params.agentLoopId) {
      errors.push("agentLoopId must be provided");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
