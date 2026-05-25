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

import type { AgentLoopResult, AgentHookTriggeredEvent } from "@wf-agent/types";
import type { ToolApprovalHandler } from "@wf-agent/types";
import { getAvailableTools } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { MetricsRegistry } from "../../../core/metrics/metrics-registry.js";
import type { GlobalContext } from "../../../core/global-context.js";
import { LLMExecutor } from "../../../core/executors/llm-executor.js";
import { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import type { CheckpointDependencies as WorkflowCheckpointDependencies } from "../../../workflow/checkpoint/checkpoint-coordinator.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import { AgentExecutionCoordinator, type AgentLoopStreamEvent } from "../coordinators/agent-execution-coordinator.js";
import { ToolExecutionCoordinator } from "../coordinators/tool-execution-coordinator.js";
import { prepareToolSchemas } from "../../../core/utils/tools/tool-schema-helper.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { buildAgentHookTriggeredCoreEvent } from "../../../core/utils/event/builders/agent-events.js";

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
  /** Tool Approval Handler (optional) */
  toolApprovalHandler?: ToolApprovalHandler;
  /** Metrics Registry (optional) */
  metricsRegistry?: MetricsRegistry;
  /** Global Context (optional, needed for builtin tool context passing) */
  globalContext?: GlobalContext;
  /** Workflow checkpoint dependencies (optional, needed for execution registry access) */
  checkpointDependencies?: WorkflowCheckpointDependencies;
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
  private toolApprovalHandler?: ToolApprovalHandler;
  private metricsRegistry?: MetricsRegistry;
  private globalContext?: GlobalContext;

  constructor(deps: AgentLoopExecutorDependencies) {
    this.llmExecutor = deps.llmExecutor;
    this.toolService = deps.toolService;
    this.eventManager = deps.eventManager;
    this.emitEvent = deps.emitEvent;
    this.toolApprovalHandler = deps.toolApprovalHandler;
    this.metricsRegistry = deps.metricsRegistry;
    this.globalContext = deps.globalContext;

    // Construct CheckpointDependencies for ToolCallExecutor if globalContext is available
    let cpDeps: WorkflowCheckpointDependencies | undefined = deps.checkpointDependencies;
    if (!cpDeps && deps.globalContext) {
      try {
        cpDeps = {
          workflowExecutionRegistry: deps.globalContext.container.get(Identifiers.WorkflowExecutionRegistry),
          checkpointStateManager: deps.globalContext.container.get(Identifiers.CheckpointState),
          workflowRegistry: deps.globalContext.container.get(Identifiers.WorkflowRegistry),
          workflowGraphRegistry: deps.globalContext.container.get(Identifiers.WorkflowGraphRegistry),
        };
      } catch {
        logger.debug("Failed to resolve checkpoint dependencies from global context");
      }
    }

    this.toolCallExecutor = new ToolCallExecutor(deps.toolService, {
      eventManager: deps.eventManager,
      checkpointDependencies: cpDeps,
      globalContext: this.globalContext,
    });
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
      toolsCount: getAvailableTools(config.availableTools).length,
      profileId: config.profileId || "DEFAULT",
      initialMessageCount: entity.conversationManager.getMessageCount(),
    });

    // Get all available tools from config (no dynamic merging)
    const toolIds = getAvailableTools(config.availableTools);

    logger.debug("Tool configuration", {
      totalCount: toolIds.length,
    });

    // Pass tool IDs directly (no filtering needed - AgentToolConfig already defines the exact tool set)
    const toolSchemas = prepareToolSchemas(toolIds, this.toolService);

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
      toolsCount: getAvailableTools(config.availableTools).length,
      profileId: config.profileId || "DEFAULT",
      initialMessageCount: entity.conversationManager.getMessageCount(),
    });

    // Get effective tools from config (no dynamic merging needed - handled by entity)
    const toolIds = getAvailableTools(config.availableTools);

    logger.debug("Tool configuration", {
      totalCount: toolIds.length,
    });

    // Pass tool IDs directly (no filtering needed - AgentToolConfig already defines the exact tool set)
    const toolSchemas = prepareToolSchemas(toolIds, this.toolService);

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
    // Create ToolExecutionCoordinator first
    const toolExecutionCoordinator = new ToolExecutionCoordinator({
      toolCallExecutor: this.toolCallExecutor,
      eventManager: this.eventManager,
      toolApprovalHandler: this.toolApprovalHandler,
    });

    // Then create AgentExecutionCoordinator with ToolExecutionCoordinator
    return new AgentExecutionCoordinator({
      llmExecutor: this.llmExecutor,
      toolExecutionCoordinator,
      emitAgentEvent: this.emitAgentEvent.bind(this),
      eventManager: this.eventManager,
      metricsRegistry: this.metricsRegistry,
    });
  }

  /**
   * Unified event emission method
   * Prioritizes EventRegistry, otherwise uses emitEvent callback
   */
  private async emitAgentEvent(event: AgentHookTriggeredEvent): Promise<void> {
    if (this.eventManager) {
      try {
        const coreEvent = buildAgentHookTriggeredCoreEvent({
          id: event.id,
          timestamp: event.timestamp,
          agentLoopId: event.agentLoopId,
          agentLoopEntityId: event.agentLoopEntityId,
          hookType: event.hookType,
          eventName: event.eventName,
          eventData: event.eventData,
          iteration: event.iteration,
          metadata: event.metadata,
        });
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
