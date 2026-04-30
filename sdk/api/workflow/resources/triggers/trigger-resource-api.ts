/**
 * TriggerResourceAPI - Trigger Resource Management API
 *  Inherits from ReadonlyResourceAPI, providing read-only operations
 */

import { ReadonlyResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { WorkflowExecutionRegistry } from "../../../../workflow/stores/workflow-execution-registry.js";
import type { Trigger } from "@wf-agent/types";
import { NotFoundError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import { getContainer } from "../../../../core/di/index.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";
import { now } from "@wf-agent/common-utils";

/**
 * Trigger Filter
 */
export interface TriggerFilter {
  /** Trigger ID list */
  ids?: string[];
  /** Trigger name (fuzzy matching is supported) */
  name?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Is it to be enabled? */
  enabled?: boolean;
}

/**
 * TriggerResourceAPI - Trigger Resource Management API
 */
export class TriggerResourceAPI extends ReadonlyResourceAPI<Trigger, string, TriggerFilter> {
  private registry: WorkflowExecutionRegistry;

  constructor() {
    super();
    const container = getContainer();
    this.registry = container.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get a single trigger
   * @param id: Trigger ID
   * @returns: Trigger object; returns null if the trigger does not exist
   */
  protected async getResource(id: string): Promise<Trigger | null> {
    // Triggers are usually obtained through workflow execution entities, and in this case, it is necessary to iterate through all workflow executions.
    const executionEntities = this.registry.getAll();
    for (const executionEntity of executionEntities) {
      const triggerManager = executionEntity.triggerManager as { getAll: () => Trigger[] } | undefined;
      const triggers = triggerManager?.getAll() || [];
      const trigger = triggers.find((t: Trigger) => t.id === id);
      if (trigger) {
        return trigger;
      }
    }
    return null;
  }

  /**
   * Get all triggers
   * @returns Array of triggers
   */
  protected async getAllResources(): Promise<Trigger[]> {
    const executionEntities = this.registry.getAll();
    const allTriggers: Trigger[] = [];

    for (const executionEntity of executionEntities) {
      const triggerManager = executionEntity.triggerManager as { getAll: () => Trigger[] } | undefined;
      const triggers = triggerManager?.getAll() || [];
      allTriggers.push(...triggers);
    }

    return allTriggers;
  }

  /**
   * Apply filter criteria
   */
  protected override applyFilter(triggers: Trigger[], filter: TriggerFilter): Trigger[] {
    return triggers.filter(trigger => {
      if (filter.ids && !filter.ids.includes(trigger.id)) {
        return false;
      }
      if (filter.name && !trigger.name.toLowerCase().includes(filter.name.toLowerCase())) {
        return false;
      }
      if (filter.workflowId && trigger.workflowId !== filter.workflowId) {
        return false;
      }
      return true;
    });
  }

  // ============================================================================
  // Trigger a specific method
  // ============================================================================

  /**
   * Get all triggers for the workflow execution
   * @param executionId: Execution ID
   * @param filter: Filter criteria
   * @returns: Array of triggers
   */
  async getWorkflowExecutionTriggers(executionId: string, filter?: TriggerFilter): Promise<Trigger[]> {
    const triggerManager = (await this.getTriggerManager(executionId)) as { getAll: () => Trigger[] };
    let triggers = triggerManager.getAll();

    // Apply filter criteria
    if (filter) {
      triggers = triggers.filter((t: Trigger) => this.applyFilter([t], filter).length > 0);
    }

    return triggers;
  }

  /**
   * Get the specified trigger for a workflow execution
   * @param executionId: Execution ID
   * @param triggerId: Trigger ID
   * @returns: Trigger object
   */
  async getWorkflowExecutionTrigger(executionId: string, triggerId: string): Promise<Trigger> {
    const triggerManager = (await this.getTriggerManager(executionId)) as {
      get: (id: string) => Trigger | undefined;
    };
    const trigger = triggerManager.get(triggerId);

    if (!trigger) {
      throw new NotFoundError(`Trigger not found: ${triggerId}`, "trigger", triggerId);
    }

    return trigger;
  }

  /**
   * Enable trigger
   * @param executionId Execution ID
   * @param triggerId Trigger ID
   */
  async enableTrigger(executionId: string, triggerId: string): Promise<void> {
    const triggerManager = (await this.getTriggerManager(executionId)) as {
      enable: (id: string) => void;
    };
    triggerManager.enable(triggerId);
  }

  /**
   * Disable the trigger
   * @param executionId Execution ID
   * @param triggerId Trigger ID
   */
  async disableTrigger(executionId: string, triggerId: string): Promise<void> {
    const triggerManager = (await this.getTriggerManager(executionId)) as {
      disable: (id: string) => void;
    };
    triggerManager.disable(triggerId);
  }

  /**
   * Check if the trigger is enabled.
   * @param executionId Execution ID
   * @param triggerId Trigger ID
   * @returns Whether it is enabled
   */
  async isTriggerEnabled(executionId: string, triggerId: string): Promise<boolean> {
    const trigger = await this.getWorkflowExecutionTrigger(executionId, triggerId);
    return trigger.status === "enabled";
  }

  /**
   * Retrieve trigger statistics
   * @param executionId Execution ID
   * @returns Statistical information
   */
  async getTriggerStatistics(executionId: string): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byType: Record<string, number>;
  }> {
    const triggers = await this.getWorkflowExecutionTriggers(executionId);

    const stats = {
      total: triggers.length,
      enabled: 0,
      disabled: 0,
      byType: {} as Record<string, number>,
    };

    for (const trigger of triggers) {
      if (trigger.status === "enabled") {
        stats.enabled++;
      } else {
        stats.disabled++;
      }

      const type = trigger.condition.eventType || "unknown";
      stats.byType[type] = (stats.byType[type] || 0) + 1;
    }

    return stats;
  }

  /**
   * Get trigger statistics for all workflow executions
   * @returns Global statistics information
   */
  async getGlobalTriggerStatistics(): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byExecution: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const executionContexts = this.registry.getAll();
    const stats = {
      total: 0,
      enabled: 0,
      disabled: 0,
      byExecution: {} as Record<string, number>,
      byType: {} as Record<string, number>,
    };

    for (const context of executionContexts) {
      const executionId = context.id;
      const triggers = (context.triggerManager as { getAll: () => Trigger[] }).getAll();

      stats.byExecution[executionId] = triggers.length;
      stats.total += triggers.length;

      for (const trigger of triggers) {
        if (trigger.status === "enabled") {
          stats.enabled++;
        } else {
          stats.disabled++;
        }

        const type = trigger.action?.type || "unknown";
        stats.byType[type] = (stats.byType[type] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Search Trigger
   * @param query Search keyword
   * @returns Array of matching triggers
   */
  async searchTriggers(query: string): Promise<Trigger[]> {
    const allTriggers = await this.getAllResources();
    return allTriggers.filter(
      trigger =>
        trigger.name.toLowerCase().includes(query.toLowerCase()) ||
        trigger.id.toLowerCase().includes(query.toLowerCase()),
    );
  }

  /**
   * Retrieve the execution history of a trigger
   * @param executionId: Execution ID
   * @param triggerId: Trigger ID
   * @returns: Array of execution histories (simplified implementation)
   */
  async getTriggerExecutionHistory(
    executionId: string,
    triggerId: string,
  ): Promise<
    Array<{
      timestamp: number;
      result: unknown;
      success: boolean;
    }>
  > {
    // Simplify the implementation; in real projects, you can obtain the necessary data from the event system.
    const trigger = await this.getWorkflowExecutionTrigger(executionId, triggerId);
    return [
      {
        timestamp: now(),
        result: `Trigger ${triggerId} is ${trigger.status}`,
        success: trigger.status === "enabled",
      },
    ];
  }

  /**
   * Export workflow execution trigger
   * @param executionId: Execution ID
   * @returns: JSON string
   */
  async exportWorkflowExecutionTriggers(executionId: string): Promise<string> {
    const triggers = await this.getWorkflowExecutionTriggers(executionId);
    return JSON.stringify(triggers, null, 2);
  }

  // ============================================================================
  // Auxiliary method
  // ============================================================================

  /**
   * Obtain the Trigger Manager
   */
  private async getTriggerManager(executionId: string) {
    const executionContext = this.registry.get(executionId);
    if (!executionContext) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }
    return executionContext.triggerManager;
  }

  /**
   * Get the underlying WorkflowExecutionRegistry instance
   * @returns WorkflowExecutionRegistry instance
   */
  getRegistry(): WorkflowExecutionRegistry {
    return this.registry;
  }
}
