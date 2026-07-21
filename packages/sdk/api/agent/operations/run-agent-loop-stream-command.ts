/**
 * RunAgentLoopStreamCommand - Run Agent Loop Stream Command
 *
 * Category: Execution (Streaming)
 * Executes an agent loop and streams events in real-time.
 * Extends StreamingExecutionBase for shared event streaming infrastructure.
 */

import {
  StreamingCommand,
  type CommandMetadataDefinition,
} from "../../shared/types/command.js";
import { validateAgentLoopRunParams } from "../../shared/operations/validators/agent-validators.js";
import type { CommandValidationResult } from "../../shared/types/command.js";
import type { AgentLoopRuntimeConfig, BaseEvent, EventType } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { ConversationSession } from "../../../shared/messaging/conversation-session.js";
import { AgentStateCoordinator } from "../../../agent/state-managers/agent-state-coordinator.js";
import {
  StreamingExecutionBase,
  type ExecutionContext,
  type EventEmitterLike,
} from "../../shared/operations/streaming-execution-base.js";

/**
 * Run Agent Loop Stream command parameters
 */
export interface RunAgentLoopStreamParams {
  /** Agent loop configuration */
  config: AgentLoopRuntimeConfig;
}

/**
 * Run Agent Loop Stream Command
 * Executes agent loop and yields events as they occur.
 * Uses the shared StreamingExecutionBase for event management.
 */
export class RunAgentLoopStreamCommand extends StreamingCommand<AsyncGenerator<BaseEvent>> {
  constructor(
    private readonly params: RunAgentLoopStreamParams,
    private readonly dependencies: APIDependencyManager,
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

  protected async executeInternal(): Promise<AsyncGenerator<BaseEvent>> {
    const streamExecutor = new AgentLoopStreamExecutor(
      this.params,
      this.dependencies,
    );
    return streamExecutor.executeStream();
  }

  validate(): CommandValidationResult {
    return validateAgentLoopRunParams(this.params.config);
  }
}

/**
 * AgentLoopStreamExecutor - Internal streaming executor for agent loop
 * Extends StreamingExecutionBase to leverage shared event streaming infrastructure.
 */
class AgentLoopStreamExecutor extends StreamingExecutionBase {
  constructor(
    private readonly params: RunAgentLoopStreamParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  protected async startExecution(): Promise<ExecutionContext> {
    const agentLoopExecutor = this.dependencies.getAgentLoopExecutor();
    const eventManager = this.dependencies.getEventManager();

    const entity = new AgentLoopEntity(`command-${Date.now()}`, this.params.config);
    const conversationSession = new ConversationSession({
      executionId: entity.id,
    });
    const stateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });

    const executionId = entity.id;

    // Create an emitter that wraps the actual event registry
    const emitter: EventEmitterLike = {
      on: (eventType: EventType, listener: (event: BaseEvent) => void) => {
        const actualEmitter = eventManager.getEmitter(executionId);
        return actualEmitter.on(eventType, listener);
      },
    };

    // Execute the agent loop stream
    const streamPromise = agentLoopExecutor.executeStream(entity, stateCoordinator);

    // Convert the generator to a promise-based execution
    const executionPromise = (async () => {
      const generator = await streamPromise;
      for await (const streamEvent of generator) {
        // Emit each stream event to the event registry so it can be captured
        // by subscribers registered via the emitter
        try {
          await eventManager.emit({
            ...streamEvent,
            executionId,
          } as unknown as BaseEvent);
        } catch {
          // Best-effort event emission
        }
      }
    })();

    return { executionId, emitter, executionPromise };
  }

  protected getEventTypes(): EventType[] {
    return [
      "AGENT_STARTED",
      "AGENT_COMPLETED",
      "AGENT_FAILED",
      "AGENT_PAUSED",
      "AGENT_RESUMED",
      "AGENT_CANCELLED",
      "AGENT_ITERATION_STARTED",
      "AGENT_ITERATION_COMPLETED",
      "AGENT_TURN_STARTED",
      "AGENT_TURN_COMPLETED",
      "AGENT_MESSAGE_STARTED",
      "AGENT_MESSAGE_COMPLETED",
      "AGENT_TOOL_EXECUTION_STARTED",
      "AGENT_TOOL_EXECUTION_COMPLETED",
      "AGENT_HOOK_TRIGGERED",
      "ERROR",
    ];
  }
}