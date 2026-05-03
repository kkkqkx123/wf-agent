/**
 * RunAgentLoopCommand - Run Agent Loop Command
 *
 * Responsibilities:
 * - Encapsulates AgentLoopCoordinator's execute() method as Command pattern
 * - Provides unified API layer interface
 * - Supports non-streaming Agent execution
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for AgentLoopCoordinator
 * - Parameter validation is completed in validate() method
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../shared/types/command.js";
import type { AgentLoopRuntimeConfig, AgentLoopResult } from "@wf-agent/types";
import { AgentLoopCoordinator } from "../../../agent/execution/coordinators/agent-loop-coordinator.js";
import type { AgentLoopEntityOptions } from "../../../agent/execution/factories/agent-loop-factory.js";

/**
 * Run Agent Loop Command Parameters
 */
export interface RunAgentLoopParams {
  /** Agent Cycle Configuration */
  config: AgentLoopRuntimeConfig;
  /** implementation option */
  options?: AgentLoopEntityOptions;
}

/**
 * Running Agent Loop Commands
 *
 * Workflow:
 * 1. validate parameters (config required)
 * 2. Execute the loop using the AgentLoopCoordinator.
 * 3. Return the AgentLoopResult result.
 */
export class RunAgentLoopCommand extends BaseCommand<AgentLoopResult> {
  constructor(
    private readonly params: RunAgentLoopParams,
    private readonly coordinator: AgentLoopCoordinator,
  ) {
    super();
  }

  protected async executeInternal(): Promise<AgentLoopResult> {
    return this.coordinator.execute(this.params.config, this.params.options);
  }
  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Validation: must provide config
    if (!this.params.config) {
      errors.push("Config must be provided.");
    }

    // Validation: profileId must be a non-empty string if provided
    if (
      this.params.config?.profileId !== undefined &&
      typeof this.params.config.profileId === "string" &&
      this.params.config.profileId.trim().length === 0
    ) {
      errors.push("`profileId` cannot be an empty string.");
    }

    // Validation: maxIterations must be a positive integer if supplied.
    if (
      this.params.config?.maxIterations !== undefined &&
      (this.params.config.maxIterations < 1 || !Number.isInteger(this.params.config.maxIterations))
    ) {
      errors.push("`maxIterations` must be a positive integer.");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
