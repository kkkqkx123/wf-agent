/**
 * RunAgentLoopStreamCommand - Run Agent Loop Stream Command
 *
 * Responsibilities:
 * - Encapsulates AgentLoopExecutor's runStream() method as Command pattern
 * - Provides unified API layer interface
 * - Supports streaming Agent execution
 *
 * Design Principles:
 * - Follows Command pattern, inherits BaseCommand
 * - Uses dependency injection for AgentLoopExecutor
 * - Returns AsyncGenerator for streaming processing
 *
 * Streaming Event Architecture:
 * - Returns AgentLoopStreamEvent, containing LLM layer events and Agent layer events
 * - LLM layer events: text, inputJson, message, etc. (from MessageStream)
 * - Agent layer events: tool_call_start/end, iteration_complete, etc.
 */

import {
  BaseCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "../../shared/types/command.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import {
  AgentLoopExecutor,
  type AgentLoopStreamEvent,
} from "../../../agent/execution/executors/agent-loop-executor.js";
import { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";

/**
 * Run the Agent with cyclic command parameters
 */
export interface RunAgentLoopStreamParams {
  /** Agent Loop Configuration */
  config: AgentLoopRuntimeConfig;
}

/**
 * Run Agent loop-based commands
 *
 * Workflow:
 * 1. Verify parameters (config is required)
 * 2. Execute the loop-based stream using AgentLoopExecutor
 * 3. Return an AsyncGenerator<AgentLoopStreamEvent>
 */
export class RunAgentLoopStreamCommand extends BaseCommand<AsyncGenerator<AgentLoopStreamEvent>> {
  constructor(
    private readonly params: RunAgentLoopStreamParams,
    private readonly agentLoopExecutor: AgentLoopExecutor,
  ) {
    super();
  }

  protected async executeInternal(): Promise<AsyncGenerator<AgentLoopStreamEvent>> {
    const entity = new AgentLoopEntity(`command-${Date.now()}`, this.params.config);
    return this.agentLoopExecutor.executeStream(entity);
  }
  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Verification: The config must be provided.
    if (!this.params.config) {
      errors.push("config must be provided");
    }

    // Verification: If `profileId` is provided, it must be a non-empty string.
    if (
      this.params.config?.profileId !== undefined &&
      typeof this.params.config.profileId === "string" &&
      this.params.config.profileId.trim().length === 0
    ) {
      errors.push("profileId cannot be an empty string");
    }

    // Verification: The `maxIterations` value, if provided, must be a positive integer.
    if (
      this.params.config?.maxIterations !== undefined &&
      (this.params.config.maxIterations < 1 || !Number.isInteger(this.params.config.maxIterations))
    ) {
      errors.push("maxIterations must be a positive integer");
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
