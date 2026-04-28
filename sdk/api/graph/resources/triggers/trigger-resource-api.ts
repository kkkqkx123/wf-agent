/**
 * TriggerResourceAPI - Trigger Resource Management API
 *  Inherits from ReadonlyResourceAPI, providing read-only operations
 */

import { ReadonlyResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { ThreadRegistry } from "../../../../graph/stores/thread-registry.js";
import type { Trigger } from "@wf-agent/types";
import { NotFoundError, ThreadContextNotFoundError } from "@wf-agent/types";
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
  private registry: ThreadRegistry;

  constructor() {
    super();
    const container = getContainer();
    this.registry = container.get(Identifiers.ThreadRegistry) as ThreadRegistry;
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
    // Triggers are usually obtained through thread entities, and in this case, it is necessary to iterate through all threads.
    const threadEntities = this.registry.getAll();
    for (const threadEntity of threadEntities) {
      const triggerManager = threadEntity.triggerManager as { getAll: () => Trigger[] } | undefined;
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
    const threadEntities = this.registry.getAll();
    const allTriggers: Trigger[] = [];

    for (const threadEntity of threadEntities) {
      const triggerManager = threadEntity.triggerManager as { getAll: () => Trigger[] } | undefined;
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
   * Get all triggers for the thread
   * @param threadId: Thread ID
   * @param filter: Filter criteria
   * @returns: Array of triggers
   */
  async getThreadTriggers(threadId: string, filter?: TriggerFilter): Promise<Trigger[]> {
    const triggerManager = (await this.getTriggerManager(threadId)) as { getAll: () => Trigger[] };
    let triggers = triggerManager.getAll();

    // Apply filter criteria
    if (filter) {
      triggers = triggers.filter((t: Trigger) => this.applyFilter([t], filter).length > 0);
    }

    return triggers;
  }

  /**
   * Get the specified trigger for a thread
   * @param threadId: Thread ID
   * @param triggerId: Trigger ID
   * @returns: Trigger object
   */
  async getThreadTrigger(threadId: string, triggerId: string): Promise<Trigger> {
    const triggerManager = (await this.getTriggerManager(threadId)) as {
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
   * @param threadId Thread ID
   * @param triggerId Trigger ID
   */
  async enableTrigger(threadId: string, triggerId: string): Promise<void> {
    const triggerManager = (await this.getTriggerManager(threadId)) as {
      enable: (id: string) => void;
    };
    triggerManager.enable(triggerId);
  }

  /**
   * Disable the trigger
   * @param threadId Thread ID
   * @param triggerId Trigger ID
   */
  async disableTrigger(threadId: string, triggerId: string): Promise<void> {
    const triggerManager = (await this.getTriggerManager(threadId)) as {
      disable: (id: string) => void;
    };
    triggerManager.disable(triggerId);
  }

  /**
   * Check if the trigger is enabled.
   * @param threadId Thread ID
   * @param triggerId Trigger ID
   * @returns Whether it is enabled
   */
  async isTriggerEnabled(threadId: string, triggerId: string): Promise<boolean> {
    const trigger = await this.getThreadTrigger(threadId, triggerId);
    return trigger.status === "enabled";
  }

  /**
   * Retrieve trigger statistics
   * @param threadId Thread ID
   * @returns Statistical information
   */
  async getTriggerStatistics(threadId: string): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byType: Record<string, number>;
  }> {
    const triggers = await this.getThreadTriggers(threadId);

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
   * Get trigger statistics for all threads
   * @returns Global statistics information
   */
  async getGlobalTriggerStatistics(): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byThread: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const threadContexts = this.registry.getAll();
    const stats = {
      total: 0,
      enabled: 0,
      disabled: 0,
      byThread: {} as Record<string, number>,
      byType: {} as Record<string, number>,
    };

    for (const context of threadContexts) {
      const threadId = context.id;
      const triggers = (context.triggerManager as { getAll: () => Trigger[] }).getAll();

      stats.byThread[threadId] = triggers.length;
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
   * @param threadId: Thread ID
   * @param triggerId: Trigger ID
   * @returns: Array of execution histories (simplified implementation)
   */
  async getTriggerExecutionHistory(
    threadId: string,
    triggerId: string,
  ): Promise<
    Array<{
      timestamp: number;
      result: unknown;
      success: boolean;
    }>
  > {
    // Simplify the implementation; in real projects, you can obtain the necessary data from the event system.
    const trigger = await this.getThreadTrigger(threadId, triggerId);
    return [
      {
        timestamp: now(),
        result: `Trigger ${triggerId} is ${trigger.status}`,
        success: trigger.status === "enabled",
      },
    ];
  }

  /**
   * Export thread trigger
   * @param threadId: Thread ID
   * @returns: JSON string
   */
  async exportThreadTriggers(threadId: string): Promise<string> {
    const triggers = await this.getThreadTriggers(threadId);
    return JSON.stringify(triggers, null, 2);
  }

  // ============================================================================
  // Auxiliary method
  // ============================================================================

  /**
   * Obtain the Trigger Manager
   */
  private async getTriggerManager(threadId: string) {
    const threadContext = this.registry.get(threadId);
    if (!threadContext) {
      throw new ThreadContextNotFoundError(`Thread not found: ${threadId}`, threadId);
    }
    return threadContext.triggerManager;
  }

  /**
   * Get the underlying ThreadRegistry instance
   * @returns ThreadRegistry instance
   */
  getRegistry(): ThreadRegistry {
    return this.registry;
  }
}
