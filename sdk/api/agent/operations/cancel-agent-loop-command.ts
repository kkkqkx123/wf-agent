/**
 * CancelAgentLoopCommand - Cancel Agent Loop Command
 *
 * Responsibilities:
 * - Encapsulates Agent Loop cancellation operation as Command pattern
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
 * Cancel the Agent loop command parameter
 */
export interface CancelAgentLoopParams {
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Reason for cancellation: */
  reason?: string;
}

/**
 * Cancel Agent Loop Command
 *
 * Workflow:
 * 1. Validate parameters (agentLoopId is required)
 * 2. Get AgentLoopEntity
 * 3. Call stop() method to cancel execution
 * 4. Return cancellation result
 */
export class CancelAgentLoopCommand extends BaseCommand<void> {
  constructor(
    private readonly params: CancelAgentLoopParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async executeInternal(): Promise<void> {
    const registry = this.dependencies.getAgentLoopRegistry();

    // Obtain the Agent Loop entity
    const entity = registry.get(this.params.agentLoopId);
    if (!entity) {
      throw new Error(`Agent Loop not found: ${this.params.agentLoopId}`);
    }

    // Check if it's possible to cancel.
    if (!entity.isRunning() && !entity.isPaused()) {
      throw new Error(`Agent Loop is not running or paused, cannot cancel`);
    }

    // Execute the cancel operation.
    entity.stop();
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Verification: The agentLoopId must be provided.
    if (!this.params.agentLoopId) {
      errors.push("agentLoopId must be provided");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
