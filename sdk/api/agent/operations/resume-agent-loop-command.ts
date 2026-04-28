/**
 * ResumeAgentLoopCommand - Resume Agent Loop Command
 *
 * Responsibilities:
 * - Encapsulates Agent Loop resume operation as Command pattern
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
 * Restore Agent loop command parameters
 */
export interface ResumeAgentLoopParams {
  /** Agent Loop ID */
  agentLoopId: ID;
}

/**
 * Resume Agent Loop Command
 *
 * Workflow:
 * 1. Validate parameters (agentLoopId is required)
 * 2. Get AgentLoopEntity
 * 3. Call resume() method to resume execution
 * 4. Return resume result
 */
export class ResumeAgentLoopCommand extends BaseCommand<void> {
  constructor(
    private readonly params: ResumeAgentLoopParams,
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

    // Check if it is possible to restore.
    if (!entity.isPaused()) {
      throw new Error(`Agent Loop is not paused, cannot resume`);
    }

    // Perform the recovery operation.
    entity.resume();
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Verification: The `agentLoopId` must be provided.
    if (!this.params.agentLoopId) {
      errors.push("Must provide agentLoopId");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
