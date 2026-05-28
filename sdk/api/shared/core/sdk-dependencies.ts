/**
 * APIDependencyManager - A class for managing API dependencies
 * It manages all Core layer dependencies required by the API layer in a unified manner.
 *
 * Design principles:
 * - Strictly control the way instances are obtained.
 * - Ensure that the API layer does not obtain instances in incorrect ways.
 * - Standardize dependency management.
 * - All methods return specific types to ensure type safety.
 * - Dependencies are uniformly obtained through a Dependency Injection (DI) container.
 */

import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { WorkflowRegistry } from "../../../workflow/stores/workflow-registry.js";
import type { WorkflowExecutionRegistry } from "../../../workflow/stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { CheckpointState } from "../../../workflow/checkpoint/checkpoint-state-manager.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import type { LLMExecutor } from "../../../core/executors/llm-executor.js";
import type { ScriptRegistry } from "../../../core/registry/script-registry.js";
import type { NodeTemplateRegistry } from "../../../core/registry/node-template-registry.js";
import type { TriggerTemplateRegistry } from "../../../core/registry/trigger-template-registry.js";
import type { WorkflowGraphRegistry } from "../../../workflow/stores/workflow-graph-registry.js";
import type { SkillRegistry } from "../../../core/registry/skill-registry.js";
import type { SkillLoader } from "../../../core/utils/skill-loader.js";
import type { AgentLoopRegistry } from "../../../agent/stores/agent-loop-registry.js";
import type { AgentLoopCoordinator } from "../../../agent/execution/coordinators/agent-loop-coordinator.js";
import type { MetricsRegistry } from "../../../core/metrics/metrics-registry.js";
import type { TaskRegistry } from "../../../workflow/stores/task/task-registry.js";
import type { CheckpointStorageAdapter, WorkflowStorageAdapter, WorkflowExecutionStorageAdapter, TaskStorageAdapter } from "@wf-agent/storage";
import type { FileCheckpointManager } from "@wf-agent/common-utils";
import type { ServiceIdentifier } from "@wf-agent/common-utils";
import type { GlobalContext } from "../../../core/global-context.js";
import type { LLMWrapper } from "../../../core/llm/wrapper.js";
import type { WorkflowLifecycleCoordinator } from "../../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js";
import type { IdBasedServiceFactory, NoArgServiceFactory } from "../../../core/di/factory-types.js";

/**
 * API Dependency Management Class
 * Manages all dependency instances through a GlobalContext instance.
 */
export class APIDependencyManager {
  /**
   * Constructor
   * @param globalContext The GlobalContext to get dependencies from
   */
  constructor(private globalContext: GlobalContext) {}

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
    return this.globalContext.container.get(
      Identifiers.WorkflowExecutionRegistry as ServiceIdentifier<WorkflowExecutionRegistry>,
    );
  }

  /**
   * Obtain the event manager
   */
  getEventManager(): EventRegistry {
    return this.globalContext.eventRegistry;
  }

  /**
   * Obtain the checkpoint status manager
   */
  getCheckpointStateManager(): CheckpointState {
    return this.globalContext.container.get(Identifiers.CheckpointState as ServiceIdentifier<CheckpointState>);
  }

  /**
   * Obtain tool services
   */
  getToolService(): ToolRegistry {
    return this.globalContext.toolRegistry;
  }

  /**
   * Obtaining the LLM executor
   */
  getLlmExecutor(): LLMExecutor {
    return this.globalContext.llmExecutor;
  }

  /**
   * Obtain code services
   */
  getScriptService(): ScriptRegistry {
    return this.globalContext.scriptRegistry;
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
   * Get the workflow graph registry
   */
  getWorkflowGraphRegistry(): WorkflowGraphRegistry {
    return this.globalContext.container.get(
      Identifiers.WorkflowGraphRegistry as ServiceIdentifier<WorkflowGraphRegistry>,
    );
  }

  /**
   * Obtain the workflow lifecycle coordinator
   *
   * WorkflowLifecycleCoordinator is registered in the DI container as an
   * IdBasedServiceFactory to support per-execution instances. This method
   * resolves the factory and creates a coordinator instance.
   */
  getWorkflowLifecycleCoordinator(): WorkflowLifecycleCoordinator {
    const factory = this.globalContext.container.get(
      Identifiers.WorkflowLifecycleCoordinator as ServiceIdentifier<
        IdBasedServiceFactory<WorkflowLifecycleCoordinator>
      >,
    );
    return (factory as unknown as IdBasedServiceFactory<WorkflowLifecycleCoordinator>).create("");
  }

  /**
   * Obtain the LLM wrapper
   */
  getLLMWrapper(): LLMWrapper {
    return this.globalContext.container.get(
      Identifiers.LLMWrapper as ServiceIdentifier<LLMWrapper>,
    );
  }

  /**
   * Get the Skill registry
   */
  getSkillRegistry(): SkillRegistry {
    return this.globalContext.container.get(Identifiers.SkillRegistry as ServiceIdentifier<SkillRegistry>);
  }

  /**
   * Obtain the Skill Loader
   */
  getSkillLoader(): SkillLoader {
    return this.globalContext.container.get(Identifiers.SkillLoader as ServiceIdentifier<SkillLoader>);
  }

  /**
   * Obtain the Agent Loop registry
   */
  getAgentLoopRegistry(): AgentLoopRegistry {
    return this.globalContext.container.get(
      Identifiers.AgentLoopRegistry as ServiceIdentifier<AgentLoopRegistry>,
    );
  }

  /**
   * Obtain the Agent Loop coordinator
   *
   * AgentLoopCoordinator is registered in the DI container as a
   * NoArgServiceFactory to support proper dependency injection. This method
   * resolves the factory and creates a coordinator instance.
   */
  getAgentLoopCoordinator(): AgentLoopCoordinator {
    const factory = this.globalContext.container.get(
      Identifiers.AgentLoopCoordinator as ServiceIdentifier<
        NoArgServiceFactory<AgentLoopCoordinator>
      >,
    );
    return (factory as unknown as NoArgServiceFactory<AgentLoopCoordinator>).create();
  }

  /**
   * Get the Task Registry
   */
  getTaskRegistry(): TaskRegistry {
    return this.globalContext.container.get(
      Identifiers.TaskRegistry as ServiceIdentifier<TaskRegistry>,
    );
  }

  /**
   * Get the Checkpoint Storage Adapter (may be null if not configured)
   */
  getCheckpointStorageAdapter(): CheckpointStorageAdapter | null {
    try {
      return this.globalContext.container.get(
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
      return this.globalContext.container.get(
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
      return this.globalContext.container.get(
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
      return this.globalContext.container.get(
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
      return this.globalContext.container.get(
        Identifiers.FileCheckpointManager as ServiceIdentifier<FileCheckpointManager>,
      ) as FileCheckpointManager | undefined;
    } catch {
      return undefined;
    }
  }

  /**
   * Get the GlobalContext instance
   */
  getGlobalContext(): GlobalContext {
    return this.globalContext;
  }

  /**
   * Get the Metrics Registry
   */
  getMetricsRegistry(): MetricsRegistry {
    return this.globalContext.container.get(
      Identifiers.MetricsRegistry as ServiceIdentifier<MetricsRegistry>,
    );
  }
}
