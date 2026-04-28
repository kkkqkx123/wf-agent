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

import { getContainer } from "../../../core/di/index.js";
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
import type { AgentLoopRegistry } from "../../../agent/loop/agent-loop-registry.js";
import type { AgentLoopCoordinator } from "../../../agent/execution/coordinators/agent-loop-coordinator.js";
import type { ServiceIdentifier } from "@wf-agent/common-utils";

/**
 * API Dependency Management Class
 * Manages all dependency instances through the DI (Dependency Injection) container.
 */
export class APIDependencyManager {
  private container = getContainer();

  /**
   * Constructor
   */
  constructor() {
    // The container has been initialized externally.
  }

  /**
   * Obtain the workflow registry
   */
  getWorkflowRegistry(): WorkflowRegistry {
    return this.container.get(Identifiers.WorkflowRegistry as ServiceIdentifier<WorkflowRegistry>);
  }

  /**
   * Get the workflow execution registry
   */
  getWorkflowExecutionRegistry(): WorkflowExecutionRegistry {
    return this.container.get(
      Identifiers.ThreadRegistry as ServiceIdentifier<WorkflowExecutionRegistry>,
    );
  }

  /**
   * Get the thread registry (alias for getWorkflowExecutionRegistry)
   * @deprecated Use getWorkflowExecutionRegistry instead
   */
  getThreadRegistry(): WorkflowExecutionRegistry {
    return this.getWorkflowExecutionRegistry();
  }

  /**
   * Obtain the event manager
   */
  getEventManager(): EventRegistry {
    return this.container.get(Identifiers.EventRegistry as ServiceIdentifier<EventRegistry>);
  }

  /**
   * Obtain the checkpoint status manager
   */
  getCheckpointStateManager(): CheckpointState {
    return this.container.get(Identifiers.CheckpointState as ServiceIdentifier<CheckpointState>);
  }

  /**
   * Obtain tool services
   */
  getToolService(): ToolRegistry {
    return this.container.get(Identifiers.ToolRegistry as ServiceIdentifier<ToolRegistry>);
  }

  /**
   * Obtaining the LLM executor
   */
  getLlmExecutor(): LLMExecutor {
    return this.container.get(Identifiers.LLMExecutor as ServiceIdentifier<LLMExecutor>);
  }

  /**
   * Obtain code services
   */
  getScriptService(): ScriptRegistry {
    return this.container.get(Identifiers.ScriptRegistry as ServiceIdentifier<ScriptRegistry>);
  }

  /**
   * Obtain the node template registry
   */
  getNodeTemplateRegistry(): NodeTemplateRegistry {
    return this.container.get(
      Identifiers.NodeTemplateRegistry as ServiceIdentifier<NodeTemplateRegistry>,
    );
  }

  /**
   * Obtain trigger template registry
   */
  getTriggerTemplateRegistry(): TriggerTemplateRegistry {
    return this.container.get(
      Identifiers.TriggerTemplateRegistry as ServiceIdentifier<TriggerTemplateRegistry>,
    );
  }

  /**
   * Get the workflow graph registry
   */
  getWorkflowGraphRegistry(): WorkflowGraphRegistry {
    return this.container.get(
      Identifiers.GraphRegistry as ServiceIdentifier<WorkflowGraphRegistry>,
    );
  }

  /**
   * Get the graph registry (alias for getWorkflowGraphRegistry)
   * @deprecated Use getWorkflowGraphRegistry instead
   */
  getGraphRegistry(): WorkflowGraphRegistry {
    return this.getWorkflowGraphRegistry();
  }

  /**
   * Obtain the workflow lifecycle coordinator
   */
  getWorkflowLifecycleCoordinator(): import("../../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js").WorkflowLifecycleCoordinator {
    return this.container.get(
      Identifiers.ThreadLifecycleCoordinator as ServiceIdentifier<
        import("../../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js").WorkflowLifecycleCoordinator
      >,
    );
  }

  /**
   * Obtain the thread lifecycle coordinator (alias for getWorkflowLifecycleCoordinator)
   * @deprecated Use getWorkflowLifecycleCoordinator instead
   */
  getThreadLifecycleCoordinator(): import("../../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js").WorkflowLifecycleCoordinator {
    return this.getWorkflowLifecycleCoordinator();
  }

  /**
   * Obtain the LLM wrapper
   */
  getLLMWrapper(): import("../../../core/llm/wrapper.js").LLMWrapper {
    return this.container.get(
      Identifiers.LLMWrapper as ServiceIdentifier<
        import("../../../core/llm/wrapper.js").LLMWrapper
      >,
    );
  }

  /**
   * Get the Skill registry
   */
  getSkillRegistry(): SkillRegistry {
    return this.container.get(Identifiers.SkillRegistry as ServiceIdentifier<SkillRegistry>);
  }

  /**
   * Obtain the Skill Loader
   */
  getSkillLoader(): SkillLoader {
    return this.container.get(Identifiers.SkillLoader as ServiceIdentifier<SkillLoader>);
  }

  /**
   * Obtain the Agent Loop registry
   */
  getAgentLoopRegistry(): AgentLoopRegistry {
    return this.container.get(
      Identifiers.AgentLoopRegistry as ServiceIdentifier<AgentLoopRegistry>,
    );
  }

  /**
   * Obtain the Agent Loop coordinator
   */
  getAgentLoopCoordinator(): AgentLoopCoordinator {
    return this.container.get(
      Identifiers.AgentLoopCoordinator as ServiceIdentifier<AgentLoopCoordinator>,
    );
  }
}
