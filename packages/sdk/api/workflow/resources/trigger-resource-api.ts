/**
 * TriggerResourceAPI - Workflow Trigger Resource Management API
 * Provides APIs for managing triggers in workflow executions.
 * Extends the shared BaseTriggerResourceAPI with workflow-specific implementation.
 */

import {
  BaseTriggerResourceAPI,
  type BaseTriggerFilter,
} from "../../shared/resources/trigger-base.js";
import type { WorkflowExecutionRegistry } from "../../../workflow/registry/workflow-execution-registry.js";
import type { Trigger } from "@wf-agent/types";
import { NotFoundError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { now } from "@wf-agent/common-utils";

/**
 * Workflow Trigger Filter
 */
export interface TriggerFilter extends BaseTriggerFilter {
  /** Trigger ID list */
  ids?: string[];
  /** Trigger name (fuzzy matching) */
  name?: string;
  /** Workflow ID filter */
  workflowId?: string;
}

/**
 * TriggerResourceAPI - Workflow Trigger Resource Management API
 */
export class TriggerResourceAPI extends BaseTriggerResourceAPI<Trigger, TriggerFilter> {
  private registry: WorkflowExecutionRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getWorkflowExecutionRegistry();
  }

  // ============================================================================
  // Implement BaseTriggerResourceAPI abstract methods
  // ============================================================================

  /**
   * Get all triggers across all workflow executions
   */
  protected getAllEntitiesTriggers(): Trigger[] {
    const executionEntities = this.registry.getAll();
    const allTriggers: Trigger[] = [];

    for (const executionEntity of executionEntities) {
      const triggerManager = executionEntity.triggerManager as
        | { getAll: () => Trigger[] }
        | undefined;
      const triggers = triggerManager?.getAll() || [];
      allTriggers.push(...triggers);
    }

    return allTriggers;
  }

  /**
   * Get triggers for a specific workflow execution
   */
  protected getEntityTriggers(executionId: string): Trigger[] {
    const executionEntity = this.registry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(
        `Workflow execution not found: ${executionId}`,
        executionId,
      );
    }

    const triggerManager = executionEntity.triggerManager as
      | { getAll: () => Trigger[] }
      | undefined;
    return triggerManager?.getAll() || [];
  }

  /**
   * Get a specific trigger for a workflow execution
   */
  protected async getEntityTrigger(executionId: string, triggerId: string): Promise<Trigger> {
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
   * Enable a trigger
   */
  protected async enableEntityTrigger(executionId: string, triggerId: string): Promise<void> {
    const triggerManager = (await this.getTriggerManager(executionId)) as {
      enable: (id: string) => void;
    };
    triggerManager.enable(triggerId);
  }

  /**
   * Disable a trigger
   */
  protected async disableEntityTrigger(executionId: string, triggerId: string): Promise<void> {
    const triggerManager = (await this.getTriggerManager(executionId)) as {
      disable: (id: string) => void;
    };
    triggerManager.disable(triggerId);
  }

  /**
   * Check if a trigger is enabled
   */
  protected async isEntityTriggerEnabled(executionId: string, triggerId: string): Promise<boolean> {
    const trigger = await this.getEntityTrigger(executionId, triggerId);
    return trigger.status === "enabled";
  }

  /**
   * Get the entity ID from a trigger
   */
  protected getEntityIdForTrigger(trigger: Trigger): string {
    return trigger.workflowId || "";
  }

  /**
   * Get the trigger ID
   */
  protected getTriggerId(trigger: Trigger): string {
    return trigger.id;
  }

  /**
   * Get the trigger name
   */
  protected getTriggerName(trigger: Trigger): string {
    return trigger.name;
  }

  /**
   * Get the trigger type
   */
  protected getTriggerType(trigger: Trigger): string {
    return trigger.action?.type || "unknown";
  }

  /**
   * Is the trigger enabled?
   */
  protected isTriggerObjectEnabled(trigger: Trigger): boolean {
    return trigger.status === "enabled";
  }

  /**
   * Apply filter criteria (workflow-specific: adds workflowId filtering)
   */
  protected override applyFilter(triggers: Trigger[], filter: TriggerFilter): Trigger[] {
    let filtered = super.applyFilter(triggers, filter);

    if (filter.workflowId) {
      filtered = filtered.filter((t) => t.workflowId === filter.workflowId);
    }

    return filtered;
  }

  // ============================================================================
  // Workflow-specific trigger methods
  // ============================================================================

  /**
   * Get all triggers for the workflow execution
   */
  async getWorkflowExecutionTriggers(
    executionId: string,
    filter?: TriggerFilter,
  ): Promise<Trigger[]> {
    return this.getEntityTriggersWithFilter(executionId, filter);
  }

  /**
   * Get the specified trigger for a workflow execution
   */
  async getWorkflowExecutionTrigger(executionId: string, triggerId: string): Promise<Trigger> {
    return this.getEntityTrigger(executionId, triggerId);
  }

  /**
   * Get trigger statistics for all workflow executions
   */
  async getWorkflowGlobalTriggerStatistics(): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byExecution: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const allTriggers = await this.getAllEntitiesTriggers();
    const stats: {
      total: number;
      enabled: number;
      disabled: number;
      byExecution: Record<string, number>;
      byType: Record<string, number>;
    } = {
      total: 0,
      enabled: 0,
      disabled: 0,
      byExecution: {},
      byType: {},
    };

    for (const trigger of allTriggers) {
      stats.total++;
      if (this.isTriggerObjectEnabled(trigger)) {
        stats.enabled++;
      } else {
        stats.disabled++;
      }
      const type = this.getTriggerType(trigger);
      stats.byType[type] = (stats.byType[type] || 0) + 1;
      const entityId = this.getEntityIdForTrigger(trigger);
      stats.byExecution[entityId] = (stats.byExecution[entityId] || 0) + 1;
    }

    return stats;
  }

  /**
   * Retrieve the execution history of a trigger
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
   */
  async exportWorkflowExecutionTriggers(executionId: string): Promise<string> {
    return this.exportEntityTriggers(executionId);
  }

  // ============================================================================
  // Auxiliary method
  // ============================================================================

  /**
   * Get the Trigger Manager
   */
  private async getTriggerManager(executionId: string) {
    const executionContext = this.registry.get(executionId);
    if (!executionContext) {
      throw new WorkflowExecutionNotFoundError(
        `Workflow execution not found: ${executionId}`,
        executionId,
      );
    }
    return executionContext.triggerManager;
  }

  /**
   * Get the underlying WorkflowExecutionRegistry instance
   */
  getRegistry(): WorkflowExecutionRegistry {
    return this.registry;
  }
}