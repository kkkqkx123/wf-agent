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
import { SerializationRegistry } from "./serialization/serialization-registry.js";
import type { WorkflowExecutionCoordinator } from "../workflow/execution/coordinators/workflow-execution-coordinator.js";
import type { WorkflowStateTransitor } from "../workflow/execution/coordinators/workflow-state-transitor.js";
import type { CheckpointCoordinator } from "../workflow/checkpoint/checkpoint-coordinator.js";
import type { WorkflowExecutionEntity } from "../workflow/entities/workflow-execution-entity.js";

/**
 * Global Context Class
 * Provides access to all shared resources for a specific SDK instance
 */
export class GlobalContext {
  // Registries (shared across all executions within this SDK instance)
  readonly workflowRegistry: WorkflowRegistry;
  readonly toolRegistry: ToolRegistry;
  readonly scriptRegistry: ScriptRegistry;
  readonly eventRegistry: EventRegistry;
  readonly nodeTemplateRegistry: NodeTemplateRegistry;
  readonly triggerTemplateRegistry: TriggerTemplateRegistry;
  
  // Execution Engines (stateless or pooled)
  readonly llmExecutor: LLMExecutor;
  readonly toolCallExecutor: ToolCallExecutor;
  readonly workflowExecutor: WorkflowExecutor;
  
  // Utilities (stateless)
  readonly serializationRegistry: SerializationRegistry;
  
  /**
   * Create a new GlobalContext instance
   * @param container The DI container to get services from
   */
  constructor(readonly container: Container) {
    // Registries
    this.workflowRegistry = container.get(Identifiers.WorkflowRegistry as ServiceIdentifier<WorkflowRegistry>);
    this.toolRegistry = container.get(Identifiers.ToolRegistry as ServiceIdentifier<ToolRegistry>);
    this.scriptRegistry = container.get(Identifiers.ScriptRegistry as ServiceIdentifier<ScriptRegistry>);
    this.eventRegistry = container.get(Identifiers.EventRegistry as ServiceIdentifier<EventRegistry>);
    this.nodeTemplateRegistry = container.get(Identifiers.NodeTemplateRegistry as ServiceIdentifier<NodeTemplateRegistry>);
    this.triggerTemplateRegistry = container.get(Identifiers.TriggerTemplateRegistry as ServiceIdentifier<TriggerTemplateRegistry>);
    
    // Executors
    this.llmExecutor = container.get(Identifiers.LLMExecutor as ServiceIdentifier<LLMExecutor>);
    this.toolCallExecutor = container.get(Identifiers.ToolCallExecutor as ServiceIdentifier<ToolCallExecutor>);
    this.workflowExecutor = container.get(Identifiers.WorkflowExecutor as ServiceIdentifier<WorkflowExecutor>);
    
    // Utilities
    this.serializationRegistry = SerializationRegistry.getInstance();
  }
  
  /**
   * Create a workflow execution coordinator for a specific execution entity
   */
  createWorkflowExecutionCoordinator(entity: WorkflowExecutionEntity): WorkflowExecutionCoordinator {
    const factory = this.container.get(Identifiers.WorkflowExecutionCoordinator) as {
      create: (executionEntity: WorkflowExecutionEntity) => WorkflowExecutionCoordinator;
    };
    return factory.create(entity);
  }
  
  /**
   * Create a state transitor for a specific execution
   */
  createStateTransitor(executionId: string): WorkflowStateTransitor {
    const factory = this.container.get(Identifiers.WorkflowStateTransitor) as {
      create: (executionId: string) => WorkflowStateTransitor;
    };
    return factory.create(executionId);
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


