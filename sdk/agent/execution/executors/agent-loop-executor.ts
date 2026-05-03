/**
 * AgentLoopExecutor - Agent Loop Executor
 *
 * Main entry point for Agent loop execution.
 * Delegates to AgentExecutionCoordinator for actual execution.
 *
 * Responsibilities:
 * - Validates configuration
 * - Creates and delegates to AgentExecutionCoordinator
 * - Manages event emission
 *
 * Does not handle:
 * - Loop iteration logic (handled by AgentExecutionCoordinator)
 * - Single iteration execution (handled by AgentIterationExecutor)
 * - Lifecycle management (handled by AgentLoopCoordinator)
 * - Error handling details (handled by ErrorHandler)
 *
 * Design Principles:
 * - Stateless design, all state managed through AgentLoopEntity
 * - Supports pause/resume functionality
 * - Supports interruption control (AbortController)
 * - Consistent architecture with WorkflowExecutor
 */

import type { AgentLoopResult, AgentHookTriggeredEvent, AgentHookTriggeredCoreEvent } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { LLMExecutor } from "../../../core/executors/llm-executor.js";
import { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import { AgentExecutionCoordinator, type AgentLoopStreamEvent } from "../coordinators/agent-execution-coordinator.js";
import { prepareToolSchemas } from "../../../core/utils/tools/tool-schema-helper.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentLoopExecutor" });

/**
 * AgentLoopExecutor Dependencies
 */
export interface AgentLoopExecutorDependencies {
  /** LLM Executor */
  llmExecutor: LLMExecutor;
  /** Tool Registry */
  toolService: ToolRegistry;
  /** Event Registry (optional) */
  eventManager?: EventRegistry;
  /** Custom event emitter (optional) */
  emitEvent?: (event: AgentHookTriggeredEvent) => Promise<void>;
}

/**
 * AgentLoopExecutor
 *
 * Lightweight executor that delegates to AgentExecutionCoordinator.
 * Follows the same pattern as WorkflowExecutor.
 */
export class AgentLoopExecutor {
  private readonly llmExecutor: LLMExecutor;
  private readonly toolService: ToolRegistry;
  private readonly toolCallExecutor: ToolCallExecutor;
  private eventManager?: EventRegistry;
  private emitEvent?: (event: AgentHookTriggeredEvent) => Promise<void>;

  constructor(deps: AgentLoopExecutorDependencies) {
    this.llmExecutor = deps.llmExecutor;
    this.toolService = deps.toolService;
    this.eventManager = deps.eventManager;
    this.emitEvent = deps.emitEvent;
    this.toolCallExecutor = new ToolCallExecutor(deps.toolService);
  }

  /**
   * Set event emitter function
   */
  setEventEmitter(emitEvent: (event: AgentHookTriggeredEvent) => Promise<void>): void {
    this.emitEvent = emitEvent;
  }

  /**
   * Set event manager
   */
  setEventManager(eventManager: EventRegistry): void {
    this.eventManager = eventManager;
  }

  /**
   * Execute Agent Loop
   *
   * @param entity Agent Loop entity
   * @returns Execution result
   */
  async execute(entity: AgentLoopEntity): Promise<AgentLoopResult> {
    const agentLoopId = entity.id;
    const config = entity.config;
    const maxIterations = config.maxIterations ?? 10;

    logger.info("Agent Loop execution started", {
      agentLoopId,
      maxIterations,
      toolsCount: config.tools?.length || 0,
      profileId: config.profileId || "DEFAULT",
      initialMessageCount: entity.conversationManager.getMessageCount(),
    });

    const toolSchemas = prepareToolSchemas(config.tools, this.toolService);

    if (toolSchemas) {
      logger.debug("Tool schemas prepared", { agentLoopId, toolsCount: toolSchemas.length });
    }

    const coordinator = this.createCoordinator();
    return coordinator.execute(
      entity,
      entity.conversationManager,
      toolSchemas,
      config.profileId || "DEFAULT",
      maxIterations,
    );
  }

  /**
   * Stream execute Agent Loop
   *
   * @param entity Agent Loop entity
   * @returns Stream event generator
   */
  async *executeStream(entity: AgentLoopEntity): AsyncGenerator<AgentLoopStreamEvent> {
    const config = entity.config;
    const maxIterations = config.maxIterations ?? 10;

    logger.info("Agent Loop stream execution started", {
      agentLoopId: entity.id,
      maxIterations,
      toolsCount: config.tools?.length || 0,
      profileId: config.profileId || "DEFAULT",
      initialMessageCount: entity.conversationManager.getMessageCount(),
    });

    const toolSchemas = prepareToolSchemas(config.tools, this.toolService);

    const coordinator = this.createCoordinator();
    yield* coordinator.executeStream(
      entity,
      entity.conversationManager,
      toolSchemas,
      config.profileId || "DEFAULT",
      maxIterations,
    );
  }

  /**
   * Create AgentExecutionCoordinator
   */
  private createCoordinator(): AgentExecutionCoordinator {
    return new AgentExecutionCoordinator({
      llmExecutor: this.llmExecutor,
      toolCallExecutor: this.toolCallExecutor,
      emitAgentEvent: this.emitAgentEvent.bind(this),
      eventManager: this.eventManager,
    });
  }

  /**
   * Unified event emission method
   * Prioritizes EventRegistry, otherwise uses emitEvent callback
   */
  private async emitAgentEvent(event: AgentHookTriggeredEvent): Promise<void> {
    if (this.eventManager) {
      try {
        const coreEvent: AgentHookTriggeredCoreEvent = {
          id: event.id,
          type: "AGENT_HOOK_TRIGGERED",
          timestamp: event.timestamp,
          agentLoopId: event.agentLoopId,
          hookType: event.hookType,
          eventName: event.eventName,
          eventData: event.eventData,
          iteration: event.iteration,
          parentWorkflowExecutionId: event.parentWorkflowExecutionId,
          nodeId: event.nodeId,
          metadata: event.metadata,
        };
        await emit(this.eventManager, coreEvent);
      } catch (error) {
        logger.debug("Failed to emit agent event", { eventType: event.type, error });
      }
    } else if (this.emitEvent) {
      await this.emitEvent(event);
    }
  }
}

export type { AgentLoopStreamEvent };
