/**
 * TriggerCoordinator - Trigger Coordinator
 * Responsible for trigger registration, deregistration, and execution of triggered actions
 *
 * Design Principles:
 * - Stateless Design: Does not maintain mutable state
 * - Coordination Logic: Encapsulates coordination logic between trigger definitions and runtime state
 * - Dependency Injection: Manages dependencies via TriggerHandlerContextFactory
 *
 * Responsibilities:
 * - Register, deregister, enable, and disable triggers
 * - Process events and execute matching triggers
 * - Query trigger definitions from WorkflowRegistry
 * - Retrieve runtime state from TriggerState
 */

import type { Trigger, TriggerStatus, WorkflowTrigger, TriggerRuntimeState } from "@wf-agent/types";
import type { BaseEvent, NodeCustomEvent, AgentHookTriggeredCoreEvent } from "@wf-agent/types";
import type { ID } from "@wf-agent/types";
import { getTriggerHandler } from "../handlers/trigger-handlers/index.js";
import { ExecutionError, RuntimeValidationError, DependencyInjectionError } from "@wf-agent/types";
import { now, getErrorOrNew } from "@wf-agent/common-utils";
import type { CheckpointDependencies } from "../../checkpoint/checkpoint-coordinator.js";
import { CheckpointCoordinator } from "../../checkpoint/checkpoint-coordinator.js";
import { convertToTrigger } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import {
  TriggerHandlerContextFactory,
  type TriggerHandlerContextFactoryConfig,
} from "../factories/trigger-handler-context-factory.js";
import type { IAgentExecutionRegistry } from "../../../agent/stores/agent-execution-registry.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";

const logger = createContextualLogger({ component: "TriggerCoordinator" });

/**
 * TriggerEventContext - Trigger event context
 *
 * Carries additional context data from the triggering event to the trigger handler.
 * Used to bridge agent hook events to triggered sub-workflow execution.
 */
export interface TriggerEventContext {
  /** Agent loop entity ID from hook triggered event */
  agentLoopEntityId: string;
}

/**
 * TriggerCoordinator - Trigger Coordinator
 *
 * Responsibilities:
 * - Registration, deregistration, activation, and deactivation of triggers
 * - Handling events and executing the corresponding triggers
 * - Querying trigger definitions from WorkflowRegistry
 * - Retrieving runtime status from TriggerState
 *
 * Design Principles:
 * - Stateless design: Does not maintain any mutable state
 * - Coordination logic: Encapsulates the logic for coordinating trigger definitions and runtime status
 * - Dependency management using TriggerHandlerContextFactory
 * - WorkflowRegistry serves as the sole source of trigger definitions
 */
export class TriggerCoordinator {
  /** Context Factory */
  private contextFactory: TriggerHandlerContextFactory;
  
  /** Error handling strategy configuration */
  private errorHandlingStrategy: 'silent' | 'log' | 'throw' = 'log';

  /**
   * Constructor (using factory configuration)
   *
   * @param config Factory configuration
   * @param options Optional configuration including error handling strategy
   */
  constructor(
    config: TriggerHandlerContextFactoryConfig,
    options?: { errorHandlingStrategy?: 'silent' | 'log' | 'throw' }
  ) {
    this.contextFactory = new TriggerHandlerContextFactory(config);
    this.errorHandlingStrategy = options?.errorHandlingStrategy || 'log';
  }

  /**
   * Obtain the context factory (for external access to dependencies)
   */
  getContextFactory(): TriggerHandlerContextFactory {
    return this.contextFactory;
  }

  /**
   * Register trigger (initialize runtime state)
   * @param workflowTrigger Workflow trigger definition
   * @param workflowId Workflow ID
   * @note Trigger configuration must be validated using validateWorkflowTrigger before calling this method
   */
  register(workflowTrigger: WorkflowTrigger, workflowId: ID): void {
    const stateManager = this.contextFactory.getStateManager();

    // Check if it already exists.
    if (stateManager.hasState(workflowTrigger.id)) {
      throw new RuntimeValidationError(`Trigger ${workflowTrigger.id} already exist`, {
        operation: "registerTrigger",
        field: "trigger.id",
        value: workflowTrigger.id,
      });
    }

    // Create a runtime state
    const state: TriggerRuntimeState = {
      triggerId: workflowTrigger.id,
      executionId: stateManager.getExecutionId(),
      workflowId: workflowId,
      status:
        workflowTrigger.enabled !== false
          ? ("enabled" as TriggerStatus)
          : ("disabled" as TriggerStatus),
      triggerCount: 0,
      updatedAt: now(),
    };

    // Registration Status
    stateManager.register(state);
  }

  /**
   * Cancel a trigger (removes the runtime state)
   * @param triggerId Trigger ID
   */
  unregister(triggerId: ID): void {
    const stateManager = this.contextFactory.getStateManager();
    if (!stateManager.hasState(triggerId)) {
      throw new ExecutionError(`Trigger ${triggerId} isn't exist`, undefined, undefined, {
        triggerId,
      });
    }

    // Delete Status
    stateManager.deleteState(triggerId);
  }

  /**
   * Enable trigger
   * @param triggerId Trigger ID
   */
  enable(triggerId: ID): void {
    const stateManager = this.contextFactory.getStateManager();
    if (!stateManager.hasState(triggerId)) {
      throw new ExecutionError(`Trigger ${triggerId} isn't exist`, undefined, undefined, {
        triggerId,
      });
    }

    const state = stateManager.getState(triggerId);
    if (state && state.status !== ("disabled" as TriggerStatus)) {
      return;
    }

    // Update status
    stateManager.updateStatus(triggerId, "enabled" as TriggerStatus);
  }

  /**
   * Disable the trigger
   * @param triggerId Trigger ID
   */
  disable(triggerId: ID): void {
    const stateManager = this.contextFactory.getStateManager();
    if (!stateManager.hasState(triggerId)) {
      throw new ExecutionError(`trigger ${triggerId} isn't exist`, undefined, undefined, {
        triggerId,
      });
    }

    const state = stateManager.getState(triggerId);
    if (state && state.status !== ("enabled" as TriggerStatus)) {
      return;
    }

    // Update status
    stateManager.updateStatus(triggerId, "disabled" as TriggerStatus);
  }

  /**
   * Get the trigger (definition + status)
   * @param triggerId Trigger ID
   * @returns The trigger; returns undefined if it does not exist
   */
  get(triggerId: ID): Trigger | undefined {
    const stateManager = this.contextFactory.getStateManager();
    // Get the status
    const state = stateManager.getState(triggerId);
    if (!state) {
      return undefined;
    }

    // Get the definition
    const workflowTrigger = this.getWorkflowTrigger(triggerId);
    if (!workflowTrigger) {
      return undefined;
    }

    // Merge definitions and states
    return this.mergeTrigger(workflowTrigger, state);
  }

  /**
   * Get all triggers (definitions + status)
   * @returns Array of triggers
   */
  getAll(): Trigger[] {
    const stateManager = this.contextFactory.getStateManager();
    // Get all statuses
    const allStates = stateManager.getAllStates();
    const triggers: Trigger[] = [];

    // Get the definitions for each state and merge them together.
    for (const [triggerId, state] of allStates.entries()) {
      const workflowTrigger = this.getWorkflowTrigger(triggerId);
      if (workflowTrigger) {
        triggers.push(this.mergeTrigger(workflowTrigger, state));
      }
    }

    return triggers;
  }

  /**
   * Handle an event (called directly by WorkflowExecutor)
   * @param event The event object
   */
  async handleEvent(event: BaseEvent): Promise<void> {
    // Get all triggers
    const triggers = this.getAll();

    // Build event context for agent hook triggered events
    let eventContext: TriggerEventContext | undefined;
    if (event.type === "AGENT_HOOK_TRIGGERED") {
      const agentEvent = event as AgentHookTriggeredCoreEvent;
      eventContext = {
        agentLoopEntityId: agentEvent.agentLoopEntityId,
      };
    }

    // Filter out triggers that are listening for this event type and have been enabled.
    const enabledTriggers = triggers.filter(
      trigger =>
        trigger.condition.eventType === event.type &&
        trigger.status === ("enabled" as TriggerStatus),
    );

    // Evaluate and execute the trigger.
    for (const trigger of enabledTriggers) {
      try {
        // Check the trigger count limit.
        if (
          trigger.maxTriggers &&
          trigger.maxTriggers > 0 &&
          (trigger.triggerCount ?? 0) >= trigger.maxTriggers
        ) {
          continue;
        }

        // Check the associations.
        if (trigger.workflowId && event.workflowId !== trigger.workflowId) {
          continue;
        }
        if (trigger.executionId && event.executionId !== trigger.executionId) {
          continue;
        }

        // For the NODE_CUSTOM_EVENT, it is necessary to additionally match the eventName.
        if (event.type === "NODE_CUSTOM_EVENT") {
          const customEvent = event as NodeCustomEvent;
          if (
            trigger.condition.eventName &&
            trigger.condition.eventName !== customEvent.eventName
          ) {
            continue;
          }
        }

        // Execute the trigger with event context
        await this.executeTrigger(trigger, eventContext);
      } catch (error) {
        // Handle errors based on configured strategy
        const errorObj = getErrorOrNew(error);
        switch (this.errorHandlingStrategy) {
          case 'silent':
            // Silently ignore errors to prevent affecting other triggers
            break;
          case 'log':
            // Log the error but continue with other triggers
            logger.warn('Trigger execution failed', {
              triggerId: trigger.id,
              triggerName: trigger.name,
              error: errorObj.message,
              eventType: event.type,
            }, undefined, errorObj);
            break;
          case 'throw':
            // Re-throw the error (will stop processing remaining triggers)
            throw error;
        }
      }
    }
  }

  /**
   * Execute the trigger
   * @param trigger The trigger
   * @param eventContext Optional event context for agent hook triggered events
   */
  private async executeTrigger(trigger: Trigger, eventContext?: TriggerEventContext): Promise<void> {
    const {
      checkpointStateManager,
      graphRegistry,
      workflowExecutionRegistry,
      workflowRegistry,
      workflowLifecycleCoordinator,
      eventManager,
    } = this.contextFactory.getDependencies();
    const stateManager = this.contextFactory.getStateManager();

    // Create a checkpoint before triggering (if configured).
    if (trigger.createCheckpoint && checkpointStateManager && trigger.executionId) {
      // If graphRegistry is not provided, skip the checkpoint creation.
      if (!graphRegistry) {
        logger.warn("WorkflowGraphRegistry not provided, skipping checkpoint creation", {
          triggerName: trigger.name,
          triggerId: trigger.id,
        });
      } else {
        try {
          const dependencies: CheckpointDependencies = {
            workflowExecutionRegistry: workflowExecutionRegistry!,
            checkpointStateManager: checkpointStateManager,
            workflowRegistry: workflowRegistry!,
            workflowGraphRegistry: graphRegistry,
          };

          await CheckpointCoordinator.createCheckpoint(
            trigger.executionId,
            dependencies,
            {
              description: trigger.checkpointDescription || `Trigger: ${trigger.name}`,
            },
          );
        } catch (error) {
          // The failure to create a checkpoint should not affect the execution of the trigger; only the error should be logged.
          logger.warn(
            "Failed to create checkpoint for trigger",
            { triggerName: trigger.name, triggerId: trigger.id },
            undefined,
            getErrorOrNew(error),
          );
        }
      }
    }

    // Use the trigger handler function to execute the trigger action.
    const handler = getTriggerHandler(trigger.action.type);

    // Pass different dependencies depending on the type of handler.

    switch (trigger.action.type) {
      case "stop_workflow_execution":
      case "pause_workflow_execution":
      case "resume_workflow_execution":
        if (!workflowLifecycleCoordinator) {
          throw new DependencyInjectionError(
            "WorkflowLifecycleCoordinator not provided",
            "WorkflowLifecycleCoordinator",
          );
        }
        await handler(trigger.action, trigger.id, workflowLifecycleCoordinator);
        break;

      case "skip_node":
        if (!workflowExecutionRegistry || !eventManager) {
          throw new DependencyInjectionError(
            "WorkflowExecutionRegistry or EventRegistry not provided",
            "WorkflowExecutionRegistry/EventRegistry",
          );
        }
        await handler(trigger.action, trigger.id, workflowExecutionRegistry, eventManager);
        break;

      case "set_variable":
      case "apply_message_operation":
        if (!workflowExecutionRegistry) {
          throw new DependencyInjectionError("WorkflowExecutionRegistry not provided", "WorkflowExecutionRegistry");
        }
        await handler(trigger.action, trigger.id, workflowExecutionRegistry);
        break;

      case "execute_triggered_subgraph":
        if (!workflowExecutionRegistry) {
          throw new DependencyInjectionError(
            "WorkflowExecutionRegistry not provided for execute_triggered_subgraph",
            "WorkflowExecutionRegistry",
          );
        }
        const globalContext = this.contextFactory.getGlobalContext();

        // Look up AgentLoopEntity from event context if available
        let agentLoopEntity: AgentLoopEntity | undefined;
        if (eventContext?.agentLoopEntityId) {
          try {
            const agentExecutionRegistry = globalContext.container.get(
              Identifiers.AgentExecutionRegistry,
            ) as IAgentExecutionRegistry | undefined;
            if (agentExecutionRegistry) {
              agentLoopEntity = await agentExecutionRegistry.get(eventContext.agentLoopEntityId);
            }
          } catch (error) {
            logger.warn('Failed to resolve agent loop entity from event context', {
              agentLoopEntityId: eventContext.agentLoopEntityId,
              triggerId: trigger.id,
            }, undefined, getErrorOrNew(error));
          }
        }

        await handler(
          trigger.action,
          trigger.id,
          workflowExecutionRegistry,
          trigger.executionId,
          globalContext,
          agentLoopEntity,
        );
        break;

      case "custom":
        // Custom handler requires containerId to resolve CustomHandlerRegistry from DI container
        await handler(trigger.action, trigger.id, trigger.executionId);
        break;

      default:
        // For other handlers, use a backward-compatible approach.
        await handler(trigger.action, trigger.id);
        break;
    }

    // Update trigger status
    stateManager.incrementTriggerCount(trigger.id);

    // If it's a one-time trigger, disable it.
    if (trigger.maxTriggers === 1) {
      stateManager.updateStatus(trigger.id, "disabled" as TriggerStatus);
    }
  }

  /**
   * Clear all trigger statuses.
   */
  clear(): void {
    const stateManager = this.contextFactory.getStateManager();
    stateManager.cleanup();
  }

  /**
   * Get workflow trigger definition
   * @param triggerId Trigger ID
   * @param workflowId Workflow ID (optional; if not provided, it will be obtained from the state manager)
   * @returns Workflow trigger definition; if not found, returns undefined
   */
  private getWorkflowTrigger(triggerId: ID, workflowId?: ID): WorkflowTrigger | undefined {
    const stateManager = this.contextFactory.getStateManager();
    const { graphRegistry } = this.contextFactory.getDependencies();

    // If the workflowId is not provided, retrieve it from the state manager.
    const targetWorkflowId = workflowId || stateManager.getWorkflowId();
    if (!targetWorkflowId) {
      return undefined;
    }

    // Using the injected graphRegistry
    if (!graphRegistry) {
      throw new DependencyInjectionError(
        "WorkflowGraphRegistry is required for trigger execution",
        "WorkflowGraphRegistry",
        "TriggerCoordinator.getWorkflowTrigger",
        undefined,
        undefined,
        { triggerId, workflowId: targetWorkflowId },
      );
    }

    const processedWorkflow = graphRegistry.get(targetWorkflowId);
    if (!processedWorkflow || !processedWorkflow.triggers) {
      return undefined;
    }

    // Find the trigger definition.
    return processedWorkflow.triggers.find((t: { id?: string }) => t.id === triggerId);
  }

  /**
   * Merge trigger definitions and state
   * @param workflowTrigger Workflow trigger definition
   * @param state Runtime state
   * @returns Complete trigger definition
   */
  private mergeTrigger(workflowTrigger: WorkflowTrigger, state: TriggerRuntimeState): Trigger {
    const stateManager = this.contextFactory.getStateManager();
    // Use `convertToTrigger` to convert to Trigger.
    const workflowId = stateManager.getWorkflowId();
    const trigger = convertToTrigger(workflowTrigger, workflowId!);

    // Merge runtime states
    trigger.status = state.status;
    trigger.triggerCount = state.triggerCount;
    trigger.executionId = state.executionId;
    trigger.updatedAt = state.updatedAt;

    return trigger;
  }
}
