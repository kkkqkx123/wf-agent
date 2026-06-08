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
import { emit } from "../../../core/utils/event/emit-event.js";
import { AgentExecutionCoordinator, type AgentLoopStreamEvent } from "../coordinators/agent-execution-coordinator.js";
import { AgentIterationCoordinator } from "../coordinators/agent-iteration-coordinator.js";
import { LLMExecutionCoordinator as CoreLLMExecutionCoordinator } from "../../../core/coordinators/llm-execution-coordinator.js";
import { ToolExecutionCoordinator } from "../coordinators/tool-execution-coordinator.js";
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
    const { toolSchemas, maxIterations, profileId } = this.prepareExecution(entity, "sync");

    const coordinator = this.createCoordinator();
    return coordinator.execute(
      entity,
      entity.conversationManager,
      toolSchemas,
      profileId,
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
    const { toolSchemas, maxIterations, profileId } = this.prepareExecution(entity, "stream");

    const coordinator = this.createCoordinator();
    yield* coordinator.executeStream(
      entity,
      entity.conversationManager,
      toolSchemas,
      profileId,
      maxIterations,
    );
  }

  /**
   * Shared preparation logic for both sync and stream execution.
   * Extracts configuration, determines defaults, prepares tool schemas,
   * and logs execution metadata.
   */
  private prepareExecution(
    entity: AgentLoopEntity,
    mode: "sync" | "stream",
  ): { toolSchemas: ReturnType<typeof prepareToolSchemas>; maxIterations: number; profileId: string } {
    const config = entity.config;
    const maxIterations = config.maxIterations ?? 10;
    const profileId = config.profileId || "DEFAULT";

    logger.info(`Agent Loop ${mode} execution started`, {
      agentLoopId: entity.id,
      maxIterations,
      toolsCount: getAvailableTools(config.availableTools).length,
      profileId,
      initialMessageCount: entity.conversationManager.getMessageCount(),
    });

    const toolIds = getAvailableTools(config.availableTools);
    logger.debug("Tool configuration", { totalCount: toolIds.length });

    const toolSchemas = prepareToolSchemas(toolIds, this.toolService);
    if (toolSchemas) {
      logger.debug("Tool schemas prepared", { agentLoopId: entity.id, toolsCount: toolSchemas.length });
    }

    return { toolSchemas, maxIterations, profileId };
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

    // Create CoreLLMExecutionCoordinator for unified LLM execution with transformContext support
    // Note: llmWrapper is a private property of LLMExecutor; we access it via `as any`
    // as a temporary workaround until LLMExecutor exposes it as a public getter.
    const coreCoordinator = new CoreLLMExecutionCoordinator(
      this.llmExecutor,
      this.toolCallExecutor,
      undefined,
      (this.llmExecutor as any)["llmWrapper"],
    );

    // Create AgentIterationCoordinator with core LLM and tool execution coordinators
    const iterationCoordinator = new AgentIterationCoordinator({
      coreCoordinator,
      toolExecutionCoordinator,
      emitAgentEvent: this.emitAgentEvent.bind(this),
      eventManager: this.eventManager,
    });

    // Create AgentExecutionCoordinator with iteration coordinator and metrics
    return new AgentExecutionCoordinator({
      iterationCoordinator,
      metricsRegistry: this.metricsRegistry,
    });
  }

  /**
   * Unified event emission method
   * Emits to EventRegistry if available, otherwise uses emitEvent callback.
   * The event is already in core format (type: "AGENT_HOOK_TRIGGERED")
   * produced by buildAgentHookTriggeredEvent.
   */
  private async emitAgentEvent(event: AgentHookTriggeredEvent): Promise<void> {
    if (this.eventManager) {
      try {
        await emit(this.eventManager, event);
      } catch (error) {
        logger.debug("Failed to emit agent event", { eventType: event.type, error });
      }
    } else if (this.emitEvent) {
      await this.emitEvent(event);
    }
  }
}

export type { AgentLoopStreamEvent };
