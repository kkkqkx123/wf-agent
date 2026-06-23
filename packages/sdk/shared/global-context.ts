/**
 * Global Context - Per-Instance Shared Resources
 *
 * Manages shared resources for a specific SDK instance:
 * - Registries (workflows, tools, scripts, events, templates)
 * - Executors (LLM, tool call, workflow)
 * - Execution pools (per-SDK-instance isolation)
 * - Utilities (serialization, parsing)
 * - Factory methods for per-execution components
 *
 * Design Principles:
 * - One instance per SDK instance (not process-wide singleton)
 * - Initialized with a specific DI container
 * - All services come from the associated container
 * - Execution pools are per-context, preventing cross-SDK pollution
 * - No global singleton state
 */

import { Container } from "@wf-agent/common-utils";
import * as Identifiers from "@sdk/di/service-identifiers.js";
import type { ServiceIdentifier } from "@wf-agent/common-utils";
import type { ExecutionEntityServiceFactory, IdBasedServiceFactory } from "@sdk/di/factory-types.js";
import { ExecutionPool, type ExecutorFactory } from "./execution/execution-pool.js";

// Import types
import type { WorkflowRegistry } from "../workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "./registry/tool-registry.js";
import type { ScriptRegistry, ScriptExecutionService } from "./registry/script-registry.js";
import type { EventRegistry } from "./registry/event-registry.js";
import type { NodeTemplateRegistry } from "./registry/node-template-registry.js";
import type { TriggerTemplateRegistry } from "./registry/trigger-template-registry.js";
import type { HookTemplateRegistry } from "./registry/hook-template-registry.js";
import type { PromptTemplateRegistry } from "./registry/prompt-template-registry.js";
import type { FragmentRegistry } from "./registry/fragment-registry.js";
import type { LLMExecutor } from "@sdk/services/executors/llm-executor.js";
import type { ToolCallExecutor } from "@sdk/services/executors/tool-call-executor.js";
import type { WorkflowExecutor } from "../workflow/execution/executors/workflow-executor.js";
import type { WorkflowExecutionCoordinator } from "../workflow/execution/coordinators/workflow-execution-coordinator.js";
import type { WorkflowStateTransitor } from "../workflow/execution/coordinators/workflow-state-transitor.js";
import { CheckpointCoordinator } from "../workflow/checkpoint/checkpoint-coordinator.js";
import type { WorkflowExecutionEntity } from "../workflow/entities/workflow-execution-entity.js";
import type { MetricsRegistry } from "@sdk/metrics/metrics-registry.js";
import type { ExecutionPoolConfig } from "./types/index.js";

/**
 * Global Context Class
 * Provides access to all shared resources for a specific SDK instance
 */
export class GlobalContext {
  // Private backing fields for lazy initialization
  private _workflowRegistry?: WorkflowRegistry;
  private _toolRegistry?: ToolRegistry;
  private _scriptRegistry?: ScriptRegistry;
  private _scriptExecutor?: ScriptExecutionService;
  private _eventRegistry?: EventRegistry;
  private _nodeTemplateRegistry?: NodeTemplateRegistry;
  private _triggerTemplateRegistry?: TriggerTemplateRegistry;
  private _hookTemplateRegistry?: HookTemplateRegistry;
  private _promptTemplateRegistry?: PromptTemplateRegistry;
  private _fragmentRegistry?: FragmentRegistry;
  private _llmExecutor?: LLMExecutor;
  private _toolCallExecutor?: ToolCallExecutor;
  private _workflowExecutor?: WorkflowExecutor;
  private _metricsRegistry?: MetricsRegistry;

  // Execution pool management - per-context isolation
  private executionPools: Map<string, ExecutionPool<unknown>> = new Map();

  /**
   * Create a new GlobalContext instance
   * @param container The DI container to get services from
   */
  constructor(readonly container: Container) {
    // All services are lazily loaded via getters to avoid circular dependency
  }

  // Lazy getters for registries
  get workflowRegistry(): WorkflowRegistry {
    if (!this._workflowRegistry) {
      this._workflowRegistry = this.container.get(
        Identifiers.WorkflowRegistry as ServiceIdentifier<WorkflowRegistry>,
      );
    }
    return this._workflowRegistry;
  }

  get toolRegistry(): ToolRegistry {
    if (!this._toolRegistry) {
      this._toolRegistry = this.container.get(
        Identifiers.ToolRegistry as ServiceIdentifier<ToolRegistry>,
      );
    }
    return this._toolRegistry;
  }

  get scriptRegistry(): ScriptRegistry {
    if (!this._scriptRegistry) {
      this._scriptRegistry = this.container.get(
        Identifiers.ScriptRegistry as ServiceIdentifier<ScriptRegistry>,
      );
    }
    return this._scriptRegistry;
  }

  get scriptExecutor(): ScriptExecutionService {
    if (!this._scriptExecutor) {
      this._scriptExecutor = this.container.get(
        Identifiers.ScriptExecutionService as ServiceIdentifier<ScriptExecutionService>,
      );
    }
    return this._scriptExecutor;
  }

  get eventRegistry(): EventRegistry {
    if (!this._eventRegistry) {
      this._eventRegistry = this.container.get(
        Identifiers.EventRegistry as ServiceIdentifier<EventRegistry>,
      );
    }
    return this._eventRegistry;
  }

  get nodeTemplateRegistry(): NodeTemplateRegistry {
    if (!this._nodeTemplateRegistry) {
      this._nodeTemplateRegistry = this.container.get(
        Identifiers.NodeTemplateRegistry as ServiceIdentifier<NodeTemplateRegistry>,
      );
    }
    return this._nodeTemplateRegistry;
  }

  get triggerTemplateRegistry(): TriggerTemplateRegistry {
    if (!this._triggerTemplateRegistry) {
      this._triggerTemplateRegistry = this.container.get(
        Identifiers.TriggerTemplateRegistry as ServiceIdentifier<TriggerTemplateRegistry>,
      );
    }
    return this._triggerTemplateRegistry;
  }

  get hookTemplateRegistry(): HookTemplateRegistry {
    if (!this._hookTemplateRegistry) {
      this._hookTemplateRegistry = this.container.get(
        Identifiers.HookTemplateRegistry as ServiceIdentifier<HookTemplateRegistry>,
      );
    }
    return this._hookTemplateRegistry;
  }

  get promptTemplateRegistry(): PromptTemplateRegistry {
    if (!this._promptTemplateRegistry) {
      this._promptTemplateRegistry = this.container.get(
        Identifiers.PromptTemplateRegistry as ServiceIdentifier<PromptTemplateRegistry>,
      );
    }
    return this._promptTemplateRegistry;
  }

  get fragmentRegistry(): FragmentRegistry {
    if (!this._fragmentRegistry) {
      this._fragmentRegistry = this.container.get(
        Identifiers.FragmentRegistry as ServiceIdentifier<FragmentRegistry>,
      );
    }
    return this._fragmentRegistry;
  }

  // Lazy getters for executors
  get llmExecutor(): LLMExecutor {
    if (!this._llmExecutor) {
      this._llmExecutor = this.container.get(
        Identifiers.LLMExecutor as ServiceIdentifier<LLMExecutor>,
      );
    }
    return this._llmExecutor;
  }

  get toolCallExecutor(): ToolCallExecutor {
    if (!this._toolCallExecutor) {
      this._toolCallExecutor = this.container.get(
        Identifiers.ToolCallExecutor as ServiceIdentifier<ToolCallExecutor>,
      );
    }
    return this._toolCallExecutor;
  }

  get workflowExecutor(): WorkflowExecutor {
    if (!this._workflowExecutor) {
      this._workflowExecutor = this.container.get(
        Identifiers.WorkflowExecutor as ServiceIdentifier<WorkflowExecutor>,
      );
    }
    return this._workflowExecutor;
  }

  get metricsRegistry(): MetricsRegistry {
    if (!this._metricsRegistry) {
      this._metricsRegistry = this.container.get(
        Identifiers.MetricsRegistry as ServiceIdentifier<MetricsRegistry>,
      );
    }
    return this._metricsRegistry;
  }

  /**
   * Create a workflow execution coordinator for a specific execution entity
   */
  createWorkflowExecutionCoordinator(
    entity: WorkflowExecutionEntity,
  ): WorkflowExecutionCoordinator {
    const factory = this.container.get(Identifiers.WorkflowExecutionCoordinator);
    return (
      factory as unknown as ExecutionEntityServiceFactory<WorkflowExecutionCoordinator>
    ).create(entity);
  }

  /**
   * Create a state transitor for a specific execution
   */
  createStateTransitor(executionId: string): WorkflowStateTransitor {
    const factory = this.container.get(Identifiers.WorkflowStateTransitor);
    return (factory as unknown as IdBasedServiceFactory<WorkflowStateTransitor>).create(
      executionId,
    );
  }

  /**
   * Create a checkpoint coordinator for a specific workflow execution
   */
  async createCheckpointCoordinator(_workflowExecutionId: string): Promise<CheckpointCoordinator> {
    const coordinator = new CheckpointCoordinator();
    return coordinator;
  }

  /**
   * Get or create an execution pool for a specific pool ID
   * Pools are per-context, ensuring multiple SDK instances don't share pools
   * @param poolId Pool identifier
   * @param executorFactory Factory for creating executors
   * @param config Pool configuration
   * @returns The execution pool instance for this context
   */
  getExecutionPool<T>(
    poolId: string,
    executorFactory: ExecutorFactory<T>,
    config?: ExecutionPoolConfig,
  ): ExecutionPool<T> {
    if (!this.executionPools.has(poolId)) {
      this.executionPools.set(poolId, new ExecutionPool(poolId, executorFactory, config));
    }
    return this.executionPools.get(poolId) as ExecutionPool<T>;
  }

  /**
   * Shutdown all execution pools and cleanup resources
   * Call this when the SDK instance is being destroyed
   */
  async shutdownExecutionPools(): Promise<void> {
    const pools = Array.from(this.executionPools.values());
    for (const pool of pools) {
      await (pool as any).shutdown?.();
    }
    this.executionPools.clear();
  }
}

