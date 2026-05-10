/**
 * Global Context - Per-Instance Shared Resources
 * 
 * Manages shared resources for a specific SDK instance:
 * - Registries (workflows, tools, scripts, events, templates)
 * - Executors (LLM, tool call, workflow)
 * - Utilities (serialization, parsing)
 * - Factory methods for per-execution components
 * 
 * Design Principles:
 * - One instance per SDK instance (not process-wide singleton)
 * - Initialized with a specific DI container
 * - All services come from the associated container
 * - No global singleton state
 */

import { Container } from "@wf-agent/common-utils";
import * as Identifiers from "./di/service-identifiers.js";
import type { ServiceIdentifier } from "@wf-agent/common-utils";
import type {
  ExecutionEntityServiceFactory,
  IdBasedServiceFactory,
} from "./di/factory-types.js";

// Import types
import type { WorkflowRegistry } from "../workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "./registry/tool-registry.js";
import type { ScriptRegistry } from "./registry/script-registry.js";
import type { EventRegistry } from "./registry/event-registry.js";
import type { NodeTemplateRegistry } from "./registry/node-template-registry.js";
import type { TriggerTemplateRegistry } from "./registry/trigger-template-registry.js";
import type { LLMExecutor } from "./executors/llm-executor.js";
import type { ToolCallExecutor } from "./executors/tool-call-executor.js";
import type { WorkflowExecutor } from "../workflow/execution/executors/workflow-executor.js";
import type { WorkflowExecutionCoordinator } from "../workflow/execution/coordinators/workflow-execution-coordinator.js";
import type { WorkflowStateTransitor } from "../workflow/execution/coordinators/workflow-state-transitor.js";
import type { CheckpointCoordinator } from "../workflow/checkpoint/checkpoint-coordinator.js";
import type { WorkflowExecutionEntity } from "../workflow/entities/workflow-execution-entity.js";

/**
 * Global Context Class
 * Provides access to all shared resources for a specific SDK instance
 */
export class GlobalContext {
  // Private backing fields for lazy initialization
  private _workflowRegistry?: WorkflowRegistry;
  private _toolRegistry?: ToolRegistry;
  private _scriptRegistry?: ScriptRegistry;
  private _eventRegistry?: EventRegistry;
  private _nodeTemplateRegistry?: NodeTemplateRegistry;
  private _triggerTemplateRegistry?: TriggerTemplateRegistry;
  private _llmExecutor?: LLMExecutor;
  private _toolCallExecutor?: ToolCallExecutor;
  private _workflowExecutor?: WorkflowExecutor;
  
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
      this._workflowRegistry = this.container.get(Identifiers.WorkflowRegistry as ServiceIdentifier<WorkflowRegistry>);
    }
    return this._workflowRegistry;
  }
  
  get toolRegistry(): ToolRegistry {
    if (!this._toolRegistry) {
      this._toolRegistry = this.container.get(Identifiers.ToolRegistry as ServiceIdentifier<ToolRegistry>);
    }
    return this._toolRegistry;
  }
  
  get scriptRegistry(): ScriptRegistry {
    if (!this._scriptRegistry) {
      this._scriptRegistry = this.container.get(Identifiers.ScriptRegistry as ServiceIdentifier<ScriptRegistry>);
    }
    return this._scriptRegistry;
  }
  
  get eventRegistry(): EventRegistry {
    if (!this._eventRegistry) {
      this._eventRegistry = this.container.get(Identifiers.EventRegistry as ServiceIdentifier<EventRegistry>);
    }
    return this._eventRegistry;
  }
  
  get nodeTemplateRegistry(): NodeTemplateRegistry {
    if (!this._nodeTemplateRegistry) {
      this._nodeTemplateRegistry = this.container.get(Identifiers.NodeTemplateRegistry as ServiceIdentifier<NodeTemplateRegistry>);
    }
    return this._nodeTemplateRegistry;
  }
  
  get triggerTemplateRegistry(): TriggerTemplateRegistry {
    if (!this._triggerTemplateRegistry) {
      this._triggerTemplateRegistry = this.container.get(Identifiers.TriggerTemplateRegistry as ServiceIdentifier<TriggerTemplateRegistry>);
    }
    return this._triggerTemplateRegistry;
  }
  
  // Lazy getters for executors
  get llmExecutor(): LLMExecutor {
    if (!this._llmExecutor) {
      this._llmExecutor = this.container.get(Identifiers.LLMExecutor as ServiceIdentifier<LLMExecutor>);
    }
    return this._llmExecutor;
  }
  
  get toolCallExecutor(): ToolCallExecutor {
    if (!this._toolCallExecutor) {
      this._toolCallExecutor = this.container.get(Identifiers.ToolCallExecutor as ServiceIdentifier<ToolCallExecutor>);
    }
    return this._toolCallExecutor;
  }
  
  get workflowExecutor(): WorkflowExecutor {
    if (!this._workflowExecutor) {
      this._workflowExecutor = this.container.get(Identifiers.WorkflowExecutor as ServiceIdentifier<WorkflowExecutor>);
    }
    return this._workflowExecutor;
  }
  
  /**
   * Create a workflow execution coordinator for a specific execution entity
   */
  createWorkflowExecutionCoordinator(entity: WorkflowExecutionEntity): WorkflowExecutionCoordinator {
    const factory = this.container.get(Identifiers.WorkflowExecutionCoordinator);
    return (factory as unknown as ExecutionEntityServiceFactory<WorkflowExecutionCoordinator>).create(entity);
  }
  
  /**
   * Create a state transitor for a specific execution
   */
  createStateTransitor(executionId: string): WorkflowStateTransitor {
    const factory = this.container.get(Identifiers.WorkflowStateTransitor);
    return (factory as unknown as IdBasedServiceFactory<WorkflowStateTransitor>).create(executionId);
  }
  
  /**
   * Create a checkpoint coordinator for a specific workflow execution
   */
  async createCheckpointCoordinator(workflowExecutionId: string): Promise<CheckpointCoordinator> {
    const coordinator = this.container.get(Identifiers.CheckpointCoordinator) as {
      createCheckpoint: (workflowExecutionId: string, metadata?: Record<string, unknown>) => Promise<CheckpointCoordinator>;
    };
    return coordinator.createCheckpoint(workflowExecutionId);
  }
}


