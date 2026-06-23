/**
 * RunAgentLoopStreamCommand - Run Agent Loop Stream Command
 *
 * Category: Execution (Streaming)
 * Executes an agent loop and streams events in real-time
 */

import {
  StreamingCommand,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
  type CommandMetadataDefinition,
} from "../../shared/types/command.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import {
  AgentLoopExecutor,
  type AgentLoopStreamEvent,
} from "../../../agent/execution/executors/agent-loop-executor.js";
import { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { ConversationSession } from "../../../shared/messaging/conversation-session.js";
import { AgentStateCoordinator } from "../../../agent/state-managers/agent-state-coordinator.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

/**
 * Run Agent Loop Stream command parameters
 */
export interface RunAgentLoopStreamParams {
  /** Agent loop configuration */
  config: AgentLoopRuntimeConfig;
}

/**
 * Run Agent Loop Stream Command
 * Executes agent loop and yields events as they occur
 */
export class RunAgentLoopStreamCommand extends StreamingCommand<AsyncGenerator<AgentLoopStreamEvent>> {
  constructor(
    private readonly params: RunAgentLoopStreamParams,
    private readonly agentLoopExecutor: AgentLoopExecutor,
  ) {
    super();
  }

  protected override getMetadataDefinition(): CommandMetadataDefinition {
    return {
      name: "RunAgentLoopStreamCommand",
      description: "Execute an agent loop and stream events in real-time",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
      supportCancellation: true,
      idempotent: false,
    };
  }

  protected async executeInternal(): Promise<AsyncGenerator<AgentLoopStreamEvent>> {
    const logger = createContextualLogger({
      component: "RunAgentLoopStreamCommand",
      commandName: "RunAgentLoopStreamCommand",
    });

    const startTime = Date.now();
    logger.info("Stream command execution started", {
      maxIterations: this.params.config?.maxIterations,
      profileId: this.params.config?.profileId,
    });

    const entity = new AgentLoopEntity(`command-${Date.now()}`, this.params.config);
    const conversationSession = new ConversationSession({
      executionId: entity.id,
    });
    const stateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });

    logger.debug("Streaming execution context initialized", undefined, {
      entityId: entity.id,
    });

    try {
      return this.agentLoopExecutor.executeStream(entity, stateCoordinator);
    } catch (error) {
      const duration = Date.now() - startTime;
      logger.error("Stream command execution failed", undefined, { duration }, error as Error);
      throw error;
    }
  }

  validate(): CommandValidationResult {
    const errors: string[] = [];

    // Verification: The config must be provided.
    if (!this.params.config) {
      errors.push("config must be provided");
    } else {
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
    }

    return errors.length > 0 ? validationFailure(errors) : validationSuccess();
  }
}
