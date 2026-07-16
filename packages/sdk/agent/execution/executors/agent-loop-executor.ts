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

import type { AgentLoopResult, AgentHookTriggeredEvent, ToolCallFormatConfig, LLMProfile } from "@wf-agent/types";
import type { Tool, ToolApprovalHandler } from "@wf-agent/types";
import { getAvailableTools } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { AgentStateCoordinator } from "../../state-managers/agent-state-coordinator.js";
import type { ToolRegistry } from "../../../shared/registry/tool-registry.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import type { MetricsRegistry } from "../../../metrics/metrics-registry.js";
import type { GlobalContext } from "../../../shared/global-context.js";
import { LLMExecutor } from "../../../services/executors/llm-executor.js";
import type { LLMWrapper } from "../../../services/llm/index.js";
import { ToolCallExecutor } from "../../../services/executors/tool-call-executor.js";
import type { CheckpointDependencies as WorkflowCheckpointDependencies } from "../../../workflow/checkpoint/checkpoint-coordinator.js";
import * as Identifiers from "../../../di/service-identifiers.js";
import { emit } from "../../../shared/events/emit-event.js";
import {
  AgentExecutionCoordinator,
  type AgentLoopStreamEvent,
} from "../coordinators/agent-execution-coordinator.js";
import { AgentIterationCoordinator } from "../coordinators/agent-iteration-coordinator.js";
import { LLMExecutionCoordinator as CoreLLMExecutionCoordinator } from "../../../shared/coordinators/llm-execution-coordinator.js";
import { ToolExecutionCoordinator } from "../coordinators/tool-execution-coordinator.js";
import { prepareToolSchemas } from "../../../shared/tools/tool-schema-helper.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { validateAgentToolCallProtocol } from "../../validation/agent-loop-validator.js";
import type { AgentLoopConfigFile } from "../../../api/shared/config/types.js";

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
          workflowExecutionRegistry: deps.globalContext.container.get(
            Identifiers.WorkflowExecutionRegistry,
          ),
          checkpointStateManager: deps.globalContext.container.get(Identifiers.CheckpointState),
          workflowRegistry: deps.globalContext.container.get(Identifiers.WorkflowRegistry),
          workflowGraphRegistry: deps.globalContext.container.get(
            Identifiers.WorkflowGraphRegistry,
          ),
        };
      } catch {
        logger.debug("Failed to resolve checkpoint dependencies from global context");
      }
    }

    this.toolCallExecutor = new ToolCallExecutor(deps.toolService, {
      eventManager: deps.eventManager,
      checkpointDependencies: cpDeps,
      globalContext: this.globalContext,
      // Note: toolFailureProtection will be injected per-execution in createCoordinator
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
   * @param stateCoordinator Agent State Coordinator for message management
   * @returns Execution result
   */
  async execute(
    entity: AgentLoopEntity,
    stateCoordinator: AgentStateCoordinator,
  ): Promise<AgentLoopResult> {
    const { toolSchemas, maxIterations, profileId } = this.prepareExecution(
      entity,
      stateCoordinator,
      "sync",
    );

    const coordinator = this.createCoordinator(stateCoordinator);
    return coordinator.execute(
      entity,
      stateCoordinator.getConversationManager(),
      toolSchemas,
      profileId,
      maxIterations,
    );
  }

  /**
   * Stream execute Agent Loop
   *
   * @param entity Agent Loop entity
   * @param stateCoordinator Agent State Coordinator for message management
   * @returns Stream event generator
   */
  async *executeStream(
    entity: AgentLoopEntity,
    stateCoordinator: AgentStateCoordinator,
  ): AsyncGenerator<AgentLoopStreamEvent> {
    const { toolSchemas, maxIterations, profileId } = this.prepareExecution(
      entity,
      stateCoordinator,
      "stream",
    );

    const coordinator = this.createCoordinator(stateCoordinator);
    yield* coordinator.executeStream(
      entity,
      stateCoordinator.getConversationManager(),
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
    stateCoordinator: AgentStateCoordinator,
    mode: "sync" | "stream",
  ): {
    toolSchemas: ReturnType<typeof prepareToolSchemas>;
    maxIterations: number;
    profileId: string;
    lockedToolCallFormat?: ToolCallFormatConfig;
  } {
    const config = entity.config;
    const maxIterations = config.maxIterations ?? 10;
    const profileId = config.profileId || "DEFAULT";

    // Resolve and lock tool call format protocol
    const resolvedProtocol = this.llmExecutor.resolveToolCallFormat(
      profileId,
      config.toolCallFormat,
    );
    entity.lockToolCallFormat(resolvedProtocol);

    // Determine the protocol source for logging/metrics
    // Priority: definition > profile > default
    const protocolSource: "definition" | "profile" | "default" =
      config.toolCallFormat !== undefined
        ? "definition"
        : this.llmExecutor["llmWrapper"]?.getProfile(profileId)?.toolCallFormat
          ? "profile"
          : "default";

    // Log protocol lock event
    logger.info("Tool call protocol locked", {
      agentLoopId: entity.id,
      profileId,
      format: resolvedProtocol.format,
      source: protocolSource,
    });

    // Record protocol lock metric
    const metricsCollector = this.metricsRegistry?.getAgentLoopCollector();
    if (metricsCollector) {
      metricsCollector.recordProtocolLocked(
        resolvedProtocol.format,
        entity.id,
        profileId,
        protocolSource,
      );
    }

    // Static pre-check: validate tool call protocol compatibility between definition and profile
    const profileResolver = (id: string): LLMProfile | undefined => {
      try {
        return this.llmExecutor["llmWrapper"]?.getProfile(id);
      } catch {
        return undefined;
      }
    };
    // Construct a minimal definition-like object for validation
    const validationDefinition: AgentLoopConfigFile = {
      id: entity.id,
      profileId: config.profileId,
      toolCallFormat: config.toolCallFormat,
    } as AgentLoopConfigFile;
    const validationResult = validateAgentToolCallProtocol(
      validationDefinition,
      profileResolver,
    );
    if (!validationResult.valid) {
      logger.warn("Agent tool call protocol validation failed", {
        agentLoopId: entity.id,
        errors: validationResult.errors,
      });
    }
    if (validationResult.warnings.length > 0) {
      logger.debug("Agent tool call protocol validation warnings", {
        agentLoopId: entity.id,
        warnings: validationResult.warnings,
      });
    }

    logger.info(`Agent Loop ${mode} execution started`, {
      agentLoopId: entity.id,
      maxIterations,
      toolsCount: getAvailableTools(config.availableTools).length,
      profileId,
      toolCallFormat: resolvedProtocol.format,
      initialMessageCount: stateCoordinator.getMessageCount(),
    });

    const toolIds = getAvailableTools(config.availableTools);
    logger.debug("Tool configuration", { totalCount: toolIds.length });

    const tools = toolIds.map(id => this.toolService.get(id)).filter((t): t is Tool => t !== undefined);
    const toolSchemas = prepareToolSchemas(tools);
    if (toolSchemas) {
      logger.debug("Tool schemas prepared", {
        agentLoopId: entity.id,
        toolsCount: toolSchemas.length,
      });
    }

    return { toolSchemas, maxIterations, profileId, lockedToolCallFormat: resolvedProtocol };
  }

  /**
   * Create AgentExecutionCoordinator
   * @param stateCoordinator Agent State Coordinator for message access
   */
  private createCoordinator(stateCoordinator: AgentStateCoordinator): AgentExecutionCoordinator {
    // Create ToolExecutionCoordinator first
    const toolExecutionCoordinator = new ToolExecutionCoordinator({
      toolCallExecutor: this.toolCallExecutor,
      eventManager: this.eventManager,
      toolApprovalHandler: this.toolApprovalHandler,
      emitEvent: this.emitEvent,
      stateCoordinator,
    });

    // Create CoreLLMExecutionCoordinator for unified LLM execution with transformContext support
    // Note: llmWrapper is a private property of LLMExecutor; we access it via `as unknown as { llmWrapper }`
    // as a temporary workaround until LLMExecutor exposes it as a public getter.
    const coreCoordinator = new CoreLLMExecutionCoordinator(
      this.llmExecutor,
      this.toolCallExecutor,
      undefined,
      (this.llmExecutor as unknown as { llmWrapper: LLMWrapper | undefined }).llmWrapper,
    );

    // Create AgentIterationCoordinator with core LLM and tool execution coordinators
    const iterationCoordinator = new AgentIterationCoordinator({
      coreCoordinator,
      toolExecutionCoordinator,
      emitAgentEvent: this.emitAgentEvent.bind(this),
      eventManager: this.eventManager,
      stateCoordinator,
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
