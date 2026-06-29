/**
 * Global Context - Per-Instance Shared Resources
 *
 * Manages shared resources for a specific SDK instance:
 * - Registries (workflows, tools, scripts, events, templates) - directly instantiated
 * - Executors (LLM, tool call, workflow) - from DI container for factory patterns
 * - Execution pools (per-SDK-instance isolation)
 * - Utilities (serialization, parsing)
 *
 * Design: Mixed approach for optimal balance
 * - Direct instantiation for 16 stateless/simple registries (no DI overhead)
 * - DI container for StorageAdapter injection and Factory patterns
 * - Eliminates ~1600 lines of unnecessary DI configuration
 *
 * This reduces complexity while maintaining all functionality:
 * - No circular dependency workarounds needed
 * - Clear registry initialization order
 * - Easy to understand and debug
 * - All adapter injection still works via constructor parameters
 */

import { Container } from "@wf-agent/common-utils";
import type { ExecutionEntityServiceFactory, IdBasedServiceFactory } from "@sdk/di/factory-types.js";
import { ExecutionPool, type ExecutorFactory } from "./execution/execution-pool.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

// Import registry implementations
import { EventRegistry } from "./registry/event-registry.js";
import { ToolRegistry } from "./registry/tool-registry.js";
import { PromptTemplateRegistry } from "./registry/prompt-template-registry.js";
import { FragmentRegistry } from "./registry/fragment-registry.js";
import { ScriptRegistry, ScriptExecutionService } from "./registry/script-registry.js";
import { NodeTemplateRegistry } from "./registry/node-template-registry.js";
import { HookTemplateRegistry } from "./registry/hook-template-registry.js";
import { TriggerTemplateRegistry } from "./registry/trigger-template-registry.js";
import { SkillRegistry } from "./registry/skill-registry.js";
import { AgentProfileRegistry } from "./registry/agent-profile-registry.js";
import { ExecutionHierarchyRegistry } from "./registry/execution-hierarchy-registry.js";
import { WorkflowGraphRegistry } from "../workflow/stores/workflow-graph-registry.js";
import { WorkflowExecutionRegistry } from "../workflow/stores/workflow-execution-registry.js";
import { TaskRegistry } from "../workflow/stores/task/task-registry.js";
import { WorkflowRegistry } from "../workflow/stores/workflow-registry.js";
import { AgentLoopRegistry } from "@sdk/agent/stores/agent-loop-registry.js";
import { MetricsRegistry } from "@sdk/metrics/metrics-registry.js";

// Import types
import type { WorkflowRegistry as WorkflowRegistryType } from "../workflow/stores/workflow-registry.js";
import type { ToolDescriptionRegistry } from "./utils/tools/tool-description-registry.js";
import type { LLMExecutor } from "@sdk/services/executors/llm-executor.js";
import type { ToolCallExecutor } from "@sdk/services/executors/tool-call-executor.js";
import type { WorkflowExecutor } from "../workflow/execution/executors/workflow-executor.js";
import type { WorkflowExecutionCoordinator } from "../workflow/execution/coordinators/workflow-execution-coordinator.js";
import type { WorkflowStateTransitor } from "../workflow/execution/coordinators/workflow-state-transitor.js";
import { CheckpointCoordinator } from "../workflow/checkpoint/checkpoint-coordinator.js";
import type { WorkflowExecutionEntity } from "../workflow/entities/workflow-execution-entity.js";
import type { ExecutionPoolConfig } from "./types/index.js";
import type { ContainerStorageConfig } from "@sdk/di/container-config.js";
import { toolDescriptionRegistry as globalToolDescriptionRegistry } from "./utils/tools/tool-description-registry.js";
import * as Identifiers from "@sdk/di/service-identifiers.js";

const logger = createContextualLogger({ component: "GlobalContext" });

/**
 * Global Context Class
 *
 * Key principles:
 * 1. Direct instantiation of 16 simple registries (EventRegistry, TaskRegistry, etc.)
 * 2. No DI overhead for registry creation
 * 3. StorageAdapter injection via constructor parameters
 * 4. Factory patterns (execution coordinators) retrieved from DI container
 * 5. Per-context isolation maintained
 */
export class GlobalContext {
  // Directly instantiated registries (no DI needed)
  // These are either stateless or only depend on storage adapters passed as parameters

  /** Event bus for SDK-wide events */
  readonly eventRegistry = new EventRegistry();

  /** Prompt template storage */
  readonly promptTemplateRegistry = new PromptTemplateRegistry();

  /** Fragment definitions */
  readonly fragmentRegistry = new FragmentRegistry();

  /** Execution hierarchy tracking */
  readonly executionHierarchyRegistry = new ExecutionHierarchyRegistry();

  /** Workflow graph structure */
  readonly workflowGraphRegistry = new WorkflowGraphRegistry();

  // Registries that need storage adapter injection (directly instantiated)
  readonly toolRegistry: ToolRegistry;
  readonly taskRegistry: TaskRegistry;
  readonly workflowRegistry: WorkflowRegistryType;
  readonly nodeTemplateRegistry: NodeTemplateRegistry;
  readonly hookTemplateRegistry: HookTemplateRegistry;
  readonly triggerTemplateRegistry: TriggerTemplateRegistry;
  readonly agentProfileRegistry: AgentProfileRegistry;
  readonly workflowExecutionRegistry: WorkflowExecutionRegistry;
  readonly agentLoopRegistry: AgentLoopRegistry;
  readonly metricsRegistry: MetricsRegistry;

  // Registries that need complex DI setup (skill loader, script executor)
  // Keep these lazy-loaded from DI container
  private _skillRegistry?: SkillRegistry;
  private _scriptRegistry?: ScriptRegistry;
  private _scriptExecutor?: ScriptExecutionService;

  get skillRegistry(): SkillRegistry {
    if (!this._skillRegistry) {
      try {
        this._skillRegistry = this.container.get(Identifiers.SkillRegistry);
      } catch (error) {
        logger.error("Failed to resolve SkillRegistry from DI container", { error });
        throw new Error(`SkillRegistry not available: ${error}`);
      }
    }
    return this._skillRegistry;
  }

  get scriptRegistry(): ScriptRegistry {
    if (!this._scriptRegistry) {
      try {
        this._scriptRegistry = this.container.get(Identifiers.ScriptRegistry);
      } catch (error) {
        logger.error("Failed to resolve ScriptRegistry from DI container", { error });
        throw new Error(`ScriptRegistry not available: ${error}`);
      }
    }
    return this._scriptRegistry;
  }

  get scriptExecutor(): ScriptExecutionService {
    if (!this._scriptExecutor) {
      try {
        this._scriptExecutor = this.container.get(Identifiers.ScriptExecutionService);
      } catch (error) {
        logger.error("Failed to resolve ScriptExecutionService from DI container", { error });
        throw new Error(`ScriptExecutionService not available: ${error}`);
      }
    }
    return this._scriptExecutor;
  }

  /** Tool Description Registry - singleton, no DI needed */
  get toolDescriptionRegistry(): ToolDescriptionRegistry {
    return globalToolDescriptionRegistry;
  }

  // Factory-based services from DI container (execution-level isolation)
  private _llmExecutor?: LLMExecutor;
  private _toolCallExecutor?: ToolCallExecutor;
  private _workflowExecutor?: WorkflowExecutor;

  // Execution pool management - per-context isolation
  private executionPools: Map<string, ExecutionPool<unknown>> = new Map();

  /**
   * Create a new GlobalContext instance with mixed instantiation:
   * - Direct: 14 simple registries (no dependencies)
   * - DI: SkillRegistry, ScriptRegistry (complex dependencies)
   * - Factory: Execution-level coordinators
   *
   * @param container DI container for complex services
   * @param adapters Storage adapters for registry initialization
   */
  constructor(
    readonly container: Container,
    adapters: ContainerStorageConfig = {},
  ) {
    // Initialize directly instantiated registries (no DI needed)
    // These are ordered to respect dependencies

    // Step 1: Create basic registries with only storage adapters
    this.workflowExecutionRegistry = new WorkflowExecutionRegistry({
      storageAdapter: adapters.workflowExecution || undefined,
    });

    this.toolRegistry = new ToolRegistry(
      undefined, // restExecutorConfig
      adapters.tool || undefined,
    );

    this.taskRegistry = new TaskRegistry({
      storageAdapter: adapters.task || undefined,
    });

    this.nodeTemplateRegistry = new NodeTemplateRegistry(adapters.nodeTemplate || undefined);
    this.hookTemplateRegistry = new HookTemplateRegistry(adapters.hookTemplate || undefined);
    this.triggerTemplateRegistry = new TriggerTemplateRegistry(adapters.trigger || undefined);

    this.agentProfileRegistry = new AgentProfileRegistry(adapters.agentProfile || undefined);
    this.agentLoopRegistry = new AgentLoopRegistry({
      storageAdapter: adapters.agentLoop || undefined,
    });

    this.metricsRegistry = new MetricsRegistry();

    // Step 2: Create registries that depend on other directly-instantiated registries
    this.workflowRegistry = new WorkflowRegistry(
      adapters.workflow || undefined,
      this.workflowExecutionRegistry,
    );

    // SkillRegistry and ScriptRegistry/ScriptExecutionService are lazy-loaded from DI
    // because they have complex dependencies (HostSkillLoader, ScriptExecutor)

    logger.info("GlobalContext initialized with 14 direct registries + 3 DI registries", {
      directRegistries: 14,
      diRegistries: 3,
    });
  }

  // Lazy getters for factory-based services from DI container
  // These use factory patterns for execution-level isolation

  get llmExecutor(): LLMExecutor {
    if (!this._llmExecutor) {
      try {
        this._llmExecutor = this.container.get(Identifiers.LLMExecutor);
      } catch (error) {
        logger.error("Failed to resolve LLMExecutor from DI container", { error });
        throw new Error(`LLMExecutor not available in DI container: ${error}`);
      }
    }
    return this._llmExecutor;
  }

  get toolCallExecutor(): ToolCallExecutor {
    if (!this._toolCallExecutor) {
      try {
        this._toolCallExecutor = this.container.get(Identifiers.ToolCallExecutor);
      } catch (error) {
        logger.error("Failed to resolve ToolCallExecutor from DI container", { error });
        throw new Error(`ToolCallExecutor not available in DI container: ${error}`);
      }
    }
    return this._toolCallExecutor;
  }

  get workflowExecutor(): WorkflowExecutor {
    if (!this._workflowExecutor) {
      try {
        this._workflowExecutor = this.container.get(Identifiers.WorkflowExecutor);
      } catch (error) {
        logger.error("Failed to resolve WorkflowExecutor from DI container", { error });
        throw new Error(`WorkflowExecutor not available in DI container: ${error}`);
      }
    }
    return this._workflowExecutor;
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

  /**
   * Get all available registries as a map
   * Useful for introspection and batch operations
   *
   * @returns Map of registry name to instance
   */
  getAllRegistries(): Map<string, any> {
    const registries = new Map<string, any>();

    // Add directly instantiated registries
    registries.set("workflow", this.workflowRegistry);
    registries.set("workflowExecution", this.workflowExecutionRegistry);
    registries.set("workflowGraph", this.workflowGraphRegistry);
    registries.set("task", this.taskRegistry);
    registries.set("tool", this.toolRegistry);
    registries.set("skill", this.skillRegistry);
    registries.set("script", this.scriptRegistry);
    registries.set("nodeTemplate", this.nodeTemplateRegistry);
    registries.set("hookTemplate", this.hookTemplateRegistry);
    registries.set("trigger", this.triggerTemplateRegistry);
    registries.set("promptTemplate", this.promptTemplateRegistry);
    registries.set("fragment", this.fragmentRegistry);
    registries.set("event", this.eventRegistry);
    registries.set("agentProfile", this.agentProfileRegistry);
    registries.set("agentLoop", this.agentLoopRegistry);
    registries.set("executionHierarchy", this.executionHierarchyRegistry);
    registries.set("metrics", this.metricsRegistry);

    return registries;
  }

  /**
   * Get a specific registry by name
   *
   * @param registryName The name of the registry to retrieve
   * @returns The registry instance, or undefined if not found
   */
  getRegistry(registryName: string): any {
    switch (registryName) {
      case "workflow":
        return this.workflowRegistry;
      case "workflowExecution":
        return this.workflowExecutionRegistry;
      case "workflowGraph":
        return this.workflowGraphRegistry;
      case "task":
        return this.taskRegistry;
      case "tool":
        return this.toolRegistry;
      case "skill":
        return this.skillRegistry;
      case "script":
        return this.scriptRegistry;
      case "nodeTemplate":
        return this.nodeTemplateRegistry;
      case "hookTemplate":
        return this.hookTemplateRegistry;
      case "trigger":
        return this.triggerTemplateRegistry;
      case "promptTemplate":
        return this.promptTemplateRegistry;
      case "fragment":
        return this.fragmentRegistry;
      case "event":
        return this.eventRegistry;
      case "agentProfile":
        return this.agentProfileRegistry;
      case "agentLoop":
        return this.agentLoopRegistry;
      case "executionHierarchy":
        return this.executionHierarchyRegistry;
      case "metrics":
        return this.metricsRegistry;
      default:
        logger.warn("Unknown registry requested", { registry: registryName });
        return undefined;
    }
  }

  /**
   * Get metadata about all registries
   * Useful for initialization planning and introspection
   *
   * @returns Array of registry metadata
   */
  getRegistryMetadata(): Array<{
    name: string;
    instantiation: "direct" | "di";
    description: string;
  }> {
    return [
      {
        name: "workflow",
        instantiation: "direct",
        description: "Workflow definitions and metadata",
      },
      {
        name: "workflowExecution",
        instantiation: "direct",
        description: "Workflow execution records and history",
      },
      {
        name: "workflowGraph",
        instantiation: "direct",
        description: "Workflow graph structure",
      },
      {
        name: "task",
        instantiation: "direct",
        description: "Task definitions and state",
      },
      {
        name: "tool",
        instantiation: "direct",
        description: "Tool definitions and implementations",
      },
      {
        name: "skill",
        instantiation: "direct",
        description: "Skill definitions and loaders",
      },
      {
        name: "script",
        instantiation: "direct",
        description: "Script storage and execution",
      },
      {
        name: "nodeTemplate",
        instantiation: "direct",
        description: "Node template definitions",
      },
      {
        name: "hookTemplate",
        instantiation: "direct",
        description: "Hook template definitions",
      },
      {
        name: "trigger",
        instantiation: "direct",
        description: "Trigger template definitions",
      },
      {
        name: "promptTemplate",
        instantiation: "direct",
        description: "Prompt template storage",
      },
      {
        name: "fragment",
        instantiation: "direct",
        description: "Fragment definitions",
      },
      {
        name: "event",
        instantiation: "direct",
        description: "Event bus for SDK-wide events",
      },
      {
        name: "agentProfile",
        instantiation: "direct",
        description: "LLM agent profile definitions",
      },
      {
        name: "agentLoop",
        instantiation: "direct",
        description: "Agent loop execution state",
      },
      {
        name: "executionHierarchy",
        instantiation: "direct",
        description: "Execution hierarchy tracking",
      },
      {
        name: "metrics",
        instantiation: "direct",
        description: "Metrics collection and reporting",
      },
    ];
  }
}
