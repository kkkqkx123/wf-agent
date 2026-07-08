/**
 * APIDependencyManager - A class for managing API dependencies
 * It manages all Core layer dependencies required by the API layer in a unified manner.
 *
 * Design principles:
 * - Strictly control the way instances are obtained.
 * - Ensure that the API layer does not obtain instances in incorrect ways.
 * - Standardize dependency management through GlobalContext.
 * - All methods return specific types to ensure type safety.
 * - Dependencies are uniformly obtained through GlobalContext getters.
 * - GlobalContext handles all DI container access internally (encapsulation).
 */

import * as Identifiers from "../../../di/service-identifiers.js";
import type { WorkflowRegistry } from "../../../workflow/stores/workflow-registry.js";
import type { WorkflowExecutionRegistry } from "../../../workflow/stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import type { CheckpointState } from "../../../workflow/checkpoint/checkpoint-state-manager.js";
import type { ToolRegistry } from "../../../shared/registry/tool-registry.js";
import type { LLMExecutor } from "../../../services/executors/llm-executor.js";
import type { ScriptRegistry, ScriptExecutionService } from "../../../shared/registry/script-registry.js";
import type { NodeTemplateRegistry } from "../../../shared/registry/node-template-registry.js";
import type { TriggerTemplateRegistry } from "../../../shared/registry/trigger-template-registry.js";
import type { HookTemplateRegistry } from "../../../shared/registry/hook-template-registry.js";
import type { WorkflowGraphRegistry } from "../../../workflow/stores/workflow-graph-registry.js";
import type { SkillRegistry } from "../../../shared/registry/skill-registry.js";
import type { AgentLoopRegistry } from "../../../agent/stores/agent-loop-registry.js";
import type { AgentLoopCoordinator } from "../../../agent/execution/coordinators/agent-loop-coordinator.js";
import type { MetricsRegistry } from "../../../metrics/metrics-registry.js";
import type { TaskRegistry } from "../../../shared/stores/task-registry.js";
import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
  TaskStorageAdapter,
} from "@wf-agent/storage";
import type { FileCheckpointManager } from "@wf-agent/common-utils";
import type { ServiceIdentifier } from "@wf-agent/common-utils";
import type { GlobalContext } from "../../../shared/global-context.js";
import type { LLMWrapper } from "../../../services/llm/wrapper.js";
import type { WorkflowLifecycleCoordinator } from "../../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js";
import type { IdBasedServiceFactory, NoArgServiceFactory } from "../../../di/factory-types.js";
import type { PersistenceLayer } from "./persistence-interfaces.js";
import { AgentLoopCheckpointResourceAPI } from "../../agent/resources/checkpoint-resource-api.js";
import { AgentPerformanceAnalysisAPI } from "../../agent/resources/agent-performance-analysis-api.js";
import { AgentErrorAnalysisAPI } from "../../agent/resources/errors/agent-error-analysis-api.js";

/**
 * API Dependency Management Class
 * Manages all dependency instances through a GlobalContext instance.
 *
 * Key Design Decision:
 * - ALL dependencies are obtained through GlobalContext getters
 * - GlobalContext encapsulates all DI container access
 * - This ensures uniform access patterns and enables GlobalContext to control caching
 */
export class APIDependencyManager {
  /**
   * Constructor
   * @param globalContext The GlobalContext to get dependencies from
   */
  constructor(private globalContext: GlobalContext) {}

  /**
   * Private helper method to access the DI container when needed
   * Note: This should only be used for dependencies not yet exposed by GlobalContext
   */
  private getFromContainer<T>(identifier: ServiceIdentifier<T>): T {
    return this.globalContext.container.get(identifier);
  }

  // ============================================================================
  // Registries - All accessed through GlobalContext getters
  // ============================================================================

  /**
   * Obtain the workflow registry
   */
  getWorkflowRegistry(): WorkflowRegistry {
    return this.globalContext.workflowRegistry;
  }

  /**
   * Get the workflow execution registry
   */
  getWorkflowExecutionRegistry(): WorkflowExecutionRegistry {
    return this.getFromContainer(Identifiers.WorkflowExecutionRegistry as ServiceIdentifier<WorkflowExecutionRegistry>);
  }

  /**
   * Obtain the event registry
   */
  getEventManager(): EventRegistry {
    return this.globalContext.eventRegistry;
  }

  /**
   * Obtain the checkpoint state manager
   */
  getCheckpointStateManager(): CheckpointState {
    return this.getFromContainer(Identifiers.CheckpointState as ServiceIdentifier<CheckpointState>);
  }

  /**
   * Obtain tool registry
   */
  getToolService(): ToolRegistry {
    return this.globalContext.toolRegistry;
  }

  /**
   * Obtain the LLM executor
   */
  getLlmExecutor(): LLMExecutor {
    return this.globalContext.llmExecutor;
  }

  /**
   * Obtain script registry (pure registry: CRUD, validation, persistence)
   */
  getScriptService(): ScriptRegistry {
    return this.globalContext.scriptRegistry;
  }

  /**
   * Obtain script executor (execution logic: execute, executeWithEngine, executeBatch, executeFlow)
   */
  getScriptExecutor(): ScriptExecutionService {
    return this.globalContext.scriptExecutor;
  }

  /**
   * Obtain the node template registry
   */
  getNodeTemplateRegistry(): NodeTemplateRegistry {
    return this.globalContext.nodeTemplateRegistry;
  }

  /**
   * Obtain trigger template registry
   */
  getTriggerTemplateRegistry(): TriggerTemplateRegistry {
    return this.globalContext.triggerTemplateRegistry;
  }

  /**
   * Obtain hook template registry
   */
  getHookTemplateRegistry(): HookTemplateRegistry {
    return this.globalContext.hookTemplateRegistry;
  }

  /**
   * Get the workflow graph registry
   */
  getWorkflowGraphRegistry(): WorkflowGraphRegistry {
    return this.getFromContainer(Identifiers.WorkflowGraphRegistry as ServiceIdentifier<WorkflowGraphRegistry>);
  }

  /**
   * Get the Metrics Registry
   */
  getMetricsRegistry(): MetricsRegistry {
    return this.getFromContainer(Identifiers.MetricsRegistry as ServiceIdentifier<MetricsRegistry>);
  }

  /**
   * Get the Skill registry
   */
  getSkillRegistry(): SkillRegistry {
    return this.getFromContainer(Identifiers.SkillRegistry as ServiceIdentifier<SkillRegistry>);
  }

  /**
   * Obtain the Agent Loop registry
   */
  getAgentLoopRegistry(): AgentLoopRegistry {
    return this.getFromContainer(Identifiers.AgentLoopRegistry as ServiceIdentifier<AgentLoopRegistry>);
  }

  /**
   * Get the Task Registry
   */
  getTaskRegistry(): TaskRegistry {
    return this.getFromContainer(Identifiers.TaskRegistry as ServiceIdentifier<TaskRegistry>);
  }

  // ============================================================================
  // Coordinators - Factory-based coordinators
  // ============================================================================

  /**
   * Obtain the workflow lifecycle coordinator
   *
   * WorkflowLifecycleCoordinator is registered in the DI container as an
   * IdBasedServiceFactory to support per-execution instances. This method
   * resolves the factory and creates a coordinator instance.
   */
  getWorkflowLifecycleCoordinator(): WorkflowLifecycleCoordinator {
    const factory = this.getFromContainer(
      Identifiers.WorkflowLifecycleCoordinator as ServiceIdentifier<
        IdBasedServiceFactory<WorkflowLifecycleCoordinator>
      >,
    );
    return (factory as unknown as IdBasedServiceFactory<WorkflowLifecycleCoordinator>).create("");
  }

  /**
   * Obtain the Agent Loop coordinator
   *
   * AgentLoopCoordinator is registered in the DI container as a
   * NoArgServiceFactory to support proper dependency injection. This method
   * resolves the factory and creates a coordinator instance.
   */
  getAgentLoopCoordinator(): AgentLoopCoordinator {
    const factory = this.getFromContainer(
      Identifiers.AgentLoopCoordinator as ServiceIdentifier<NoArgServiceFactory<AgentLoopCoordinator>>,
    );
    return (factory as unknown as NoArgServiceFactory<AgentLoopCoordinator>).create();
  }

  // ============================================================================
  // Wrappers
  // ============================================================================

  /**
   * Obtain the LLM wrapper
   */
  getLLMWrapper(): LLMWrapper {
    return this.getFromContainer(Identifiers.LLMWrapper as ServiceIdentifier<LLMWrapper>);
  }

  // ============================================================================
  // Storage Adapters - Optional adapters with graceful fallback
  // ============================================================================

  /**
   * Get the Checkpoint Storage Adapter (may be null if not configured)
   */
  getCheckpointStorageAdapter(): CheckpointStorageAdapter | null {
    try {
      return this.getFromContainer(
        Identifiers.CheckpointStorageAdapter as ServiceIdentifier<CheckpointStorageAdapter>,
      ) as CheckpointStorageAdapter | null;
    } catch {
      return null;
    }
  }

  /**
   * Get the Workflow Storage Adapter (may be null if not configured)
   */
  getWorkflowStorageAdapter(): WorkflowStorageAdapter | null {
    try {
      return this.getFromContainer(
        Identifiers.WorkflowStorageAdapter as ServiceIdentifier<WorkflowStorageAdapter>,
      ) as WorkflowStorageAdapter | null;
    } catch {
      return null;
    }
  }

  /**
   * Get the Workflow Execution Storage Adapter (may be null if not configured)
   */
  getWorkflowExecutionStorageAdapter(): WorkflowExecutionStorageAdapter | null {
    try {
      return this.getFromContainer(
        Identifiers.WorkflowExecutionStorageAdapter as ServiceIdentifier<WorkflowExecutionStorageAdapter>,
      ) as WorkflowExecutionStorageAdapter | null;
    } catch {
      return null;
    }
  }

  /**
   * Get the Task Storage Adapter (may be null if not configured)
   */
  getTaskStorageAdapter(): TaskStorageAdapter | null {
    try {
      return this.getFromContainer(
        Identifiers.TaskStorageAdapter as ServiceIdentifier<TaskStorageAdapter>,
      ) as TaskStorageAdapter | null;
    } catch {
      return null;
    }
  }

  /**
   * Get the FileCheckpointManager (may be undefined if not configured)
   */
  getFileCheckpointManager(): FileCheckpointManager | undefined {
    try {
      return this.getFromContainer(
        Identifiers.FileCheckpointManager as ServiceIdentifier<FileCheckpointManager>,
      ) as FileCheckpointManager | undefined;
    } catch {
      return undefined;
    }
  }

  /**
   * Get the Persistence Layer (may be null if not configured)
   */
  getPersistenceLayer(): PersistenceLayer | null {
    try {
      return this.getFromContainer(
        Identifiers.PersistenceLayer as ServiceIdentifier<PersistenceLayer>,
      ) as PersistenceLayer | null;
    } catch {
      return null;
    }
  }

  // ============================================================================
  // Analysis APIs
  // ============================================================================

  /**
   * Get the Agent Performance Analysis API
   * Provides performance profiling and bottleneck identification
   */
  getAgentPerformanceAnalysisAPI(): AgentPerformanceAnalysisAPI {
    return new AgentPerformanceAnalysisAPI(this);
  }

  /**
   * Get the Agent Error Analysis API
   * Provides error statistics and advanced error analysis
   */
  getAgentErrorAnalysisAPI(): AgentErrorAnalysisAPI {
    return new AgentErrorAnalysisAPI(this);
  }

  // ============================================================================
  // Resource APIs
  // ============================================================================

  /**
   * Get the Agent Loop Checkpoint Resource API
   * Provides checkpoint CRUD operations for agent loops
   */
  getAgentLoopCheckpointResourceAPI(): AgentLoopCheckpointResourceAPI {
    return new AgentLoopCheckpointResourceAPI();
  }

  // ============================================================================
  // Context Access
  // ============================================================================

  /**
   * Get the GlobalContext instance
   */
  getGlobalContext(): GlobalContext {
    return this.globalContext;
  }
}
