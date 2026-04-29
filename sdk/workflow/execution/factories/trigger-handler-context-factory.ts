/**
 * Trigger Processor Context Factory
 * Responsible for creating the appropriate processor context for different types of trigger actions
 *
 * Design Principles:
 * - Centralized management of trigger processor dependencies
 * - Creation of the right context based on the type of trigger action
 * - Simplification of the responsibilities of TriggerCoordinator
 * - Verification of the presence of necessary dependencies
 * - Each method receives only the parameters it actually needs
 */

import type { Trigger, TriggerAction } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { WorkflowRegistry } from "../../stores/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../../stores/workflow-graph-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { TriggerState } from "../../state-managers/trigger-state.js";
import type { CheckpointState } from "../../checkpoint/checkpoint-state-manager.js";
import type { WorkflowExecutionBuilder } from "./workflow-execution-builder.js";
import type { TaskQueue } from "../../stores/task/task-queue.js";
import type { WorkflowStateTransitor } from "../coordinators/workflow-state-transitor.js";
import { DependencyInjectionError } from "@wf-agent/types";

/**
 * Lifecycle Trigger Context
 * Used for stop_thread, pause_thread, resume_thread actions
 */
export interface LifecycleTriggerContext {
  threadLifecycleCoordinator: WorkflowStateTransitor;
}

/**
 * Skip node trigger context
 * Used for the skip_node action
 */
export interface SkipNodeTriggerContext {
  workflowExecutionRegistry: WorkflowExecutionRegistry;
  eventManager: EventRegistry;
}

/**
 * Set Variable Trigger Context
 * Used for the set_variable action
 */
export interface SetVariableTriggerContext {
  workflowExecutionRegistry: WorkflowExecutionRegistry;
}

/**
 * Execute Subgraph Trigger Context
 * Used for the execute_triggered_subgraph action
 */
export interface ExecuteSubgraphTriggerContext {
  workflowExecutionRegistry: WorkflowExecutionRegistry;
  eventManager: EventRegistry;
  threadBuilder: WorkflowExecutionBuilder;
  taskQueueManager: TaskQueue;
  parentThreadId: string;
}

/**
 * Trigger Processor Context Factory Configuration
 */
export interface TriggerHandlerContextFactoryConfig {
  // Core Dependencies (Required)
  /** Workflow Execution Registry */
  workflowExecutionRegistry: WorkflowExecutionRegistry;
  /** Workflow Registry */
  workflowRegistry: WorkflowRegistry;
  /** Trigger State Manager */
  stateManager: TriggerState;

  // Optional dependencies
  /** Checkpoint State Manager */
  checkpointStateManager?: CheckpointState;
  /** Workflow Graph Registry */
  graphRegistry?: WorkflowGraphRegistry;
  /** Event Manager */
  eventManager?: EventRegistry;
  /** Workflow Execution Builder */
  threadBuilder?: WorkflowExecutionBuilder;
  /** Task Queue Manager */
  taskQueueManager?: TaskQueue;
  /** Workflow State Transitor */
  threadLifecycleCoordinator?: WorkflowStateTransitor;
}

/**
 * Trigger Processor Context Factory
 *
 * Responsibilities:
 * - Creates the appropriate processor context based on the type of trigger action.
 * - Manages the dependencies of trigger processors in a centralized manner.
 * - Ensures that all necessary dependencies are in place.
 */
export class TriggerHandlerContextFactory {
  constructor(private config: TriggerHandlerContextFactoryConfig) {}

  /**
   * Create a trigger processor context
   *
   * @param trigger The trigger
   * @returns The processor context
   * @throws DependencyInjectionError When a required dependency is missing
   */
  createHandlerContext(trigger: Trigger): TriggerHandlerContext {
    const actionType = trigger.action.type;

    switch (actionType) {
      case "stop_thread":
      case "pause_thread":
      case "resume_thread":
        return this.createLifecycleContext(trigger.id, trigger.action.type);

      case "skip_node":
        return this.createSkipNodeContext(trigger.id, trigger.action.type);

      case "set_variable":
        return this.createSetVariableContext();

      case "execute_triggered_subgraph":
        return this.createSubgraphContext(trigger.id, trigger.action.type, trigger.threadId);

      default:
        // For other action types, return an empty context.
        return {};
    }
  }

  /**
   * Create a lifecycle trigger context
   *
   * @param triggerId The trigger ID (for error reporting)
   * @param actionType The action type (for error reporting)
   * @returns The lifecycle trigger context
   * @throws DependencyInjectionError When the WorkflowLifecycleCoordinator is missing
   */
  private createLifecycleContext(triggerId: string, actionType: string): LifecycleTriggerContext {
    if (!this.config.threadLifecycleCoordinator) {
      throw new DependencyInjectionError(
        "WorkflowLifecycleCoordinator is required for lifecycle trigger actions",
        "WorkflowLifecycleCoordinator",
        "TriggerHandlerContextFactory.createLifecycleContext",
        undefined,
        undefined,
        { triggerId, actionType },
      );
    }

    return {
      threadLifecycleCoordinator: this.config.threadLifecycleCoordinator,
    };
  }

  /**
   * Create a context for the skip node trigger
   *
   * @param triggerId The trigger ID (for error reporting)
   * @param actionType The action type (for error reporting)
   * @returns The context for the skip node trigger
   * @throws DependencyInjectionError When a required dependency is missing
   */
  private createSkipNodeContext(triggerId: string, actionType: string): SkipNodeTriggerContext {
    if (!this.config.eventManager) {
      throw new DependencyInjectionError(
        "EventRegistry is required for skip_node trigger action",
        "EventRegistry",
        "TriggerHandlerContextFactory.createSkipNodeContext",
        undefined,
        undefined,
        { triggerId, actionType },
      );
    }

    return {
      workflowExecutionRegistry: this.config.workflowExecutionRegistry,
      eventManager: this.config.eventManager,
    };
  }

  /**
   * Create a context for setting variable triggers
   *
   * @returns The context for setting variable triggers
   */
  private createSetVariableContext(): SetVariableTriggerContext {
    return {
      workflowExecutionRegistry: this.config.workflowExecutionRegistry,
    };
  }

  /**
   * Create the execution subgraph trigger context
   *
   * @param triggerId The trigger ID (for error reporting)
   * @param actionType The action type (for error reporting)
   * @param parentThreadId The parent thread ID
   * @returns The execution subgraph trigger context
   * @throws DependencyInjectionError When a required dependency is missing
   */
  private createSubgraphContext(
    triggerId: string,
    actionType: string,
    parentThreadId: string | undefined,
  ): ExecuteSubgraphTriggerContext {
    if (!this.config.eventManager) {
      throw new DependencyInjectionError(
        "EventRegistry is required for execute_triggered_subgraph trigger action",
        "EventRegistry",
        "TriggerHandlerContextFactory.createSubgraphContext",
        undefined,
        undefined,
        { triggerId, actionType },
      );
    }

    if (!this.config.threadBuilder) {
      throw new DependencyInjectionError(
        "WorkflowExecutionBuilder is required for execute_triggered_subgraph trigger action",
        "WorkflowExecutionBuilder",
        "TriggerHandlerContextFactory.createSubgraphContext",
        undefined,
        undefined,
        { triggerId, actionType },
      );
    }

    if (!this.config.taskQueueManager) {
      throw new DependencyInjectionError(
        "TaskQueue is required for execute_triggered_subgraph trigger action",
        "TaskQueue",
        "TriggerHandlerContextFactory.createSubgraphContext",
        undefined,
        undefined,
        { triggerId, actionType },
      );
    }

    return {
      workflowExecutionRegistry: this.config.workflowExecutionRegistry,
      eventManager: this.config.eventManager,
      threadBuilder: this.config.threadBuilder,
      taskQueueManager: this.config.taskQueueManager,
      parentThreadId: parentThreadId || "",
    };
  }

  /**
   * Check if the checkpoint feature is supported
   *
   * @returns Whether it is supported
   */
  hasCheckpointSupport(): boolean {
    return !!(this.config.checkpointStateManager && this.config.graphRegistry);
  }

  /**
   * Get checkpoint state manager
   *
   * @returns Checkpoint state manager
   * @throws DependencyInjectionError When checkpoint support is not available
   */
  getCheckpointStateManager(): CheckpointState {
    if (!this.config.checkpointStateManager) {
      throw new DependencyInjectionError(
        "CheckpointState is not configured",
        "CheckpointState",
        "TriggerHandlerContextFactory.getCheckpointStateManager",
      );
    }
    return this.config.checkpointStateManager;
  }

  /**
   * Get graph registry
   *
   * @returns Graph registry
   * @throws DependencyInjectionError When checkpoint support is not available
   */
  getGraphRegistry(): WorkflowGraphRegistry {
    if (!this.config.graphRegistry) {
      throw new DependencyInjectionError(
        "WorkflowGraphRegistry is not configured",
        "WorkflowGraphRegistry",
        "TriggerHandlerContextFactory.getGraphRegistry",
      );
    }
    return this.config.graphRegistry;
  }

  /**
   * Get state manager
   *
   * @returns Trigger state manager
   */
  getStateManager(): TriggerState {
    return this.config.stateManager;
  }

  /**
   * Get dependencies
   *
   * @returns Dependencies configuration
   */
  getDependencies(): TriggerHandlerContextFactoryConfig {
    return this.config;
  }
}

/**
 * Trigger handler context union type
 */
export type TriggerHandlerContext =
  | LifecycleTriggerContext
  | SkipNodeTriggerContext
  | SetVariableTriggerContext
  | ExecuteSubgraphTriggerContext
  | Record<string, never>;
