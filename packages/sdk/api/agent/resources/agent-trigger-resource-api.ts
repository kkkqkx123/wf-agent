/**
 * AgentTriggerResourceAPI - Agent Trigger Resource Management API
 * Inherits from QueryableResourceAPI, providing read-only operations
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { AgentLoopRegistry } from "../../../agent/stores/agent-loop-registry.js";
import type { AgentTrigger } from "@wf-agent/types";
import { NotFoundError, type ID } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Agent Trigger Filter
 */
export interface AgentTriggerFilter {
  /** Trigger ID list */
  ids?: string[];
  /** Trigger name (fuzzy matching is supported) */
  name?: string;
  /** Agent loop ID */
  agentLoopId?: ID;
  /** Is it enabled? */
  enabled?: boolean;
  /** Trigger type */
  triggerType?: string;
}

/**
 * AgentTriggerResourceAPI - Agent Trigger Resource Management API
 */
export class AgentTriggerResourceAPI extends QueryableResourceAPI<AgentTrigger, string, AgentTriggerFilter> {
  private registry: AgentLoopRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getAgentLoopRegistry();
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get a single trigger
   * @param id: Trigger ID
   * @returns: Trigger object; returns null if the trigger does not exist
   */
  protected async getResource(id: string): Promise<AgentTrigger | null> {
    // Triggers are usually obtained through agent loop entities
    const agentLoops = this.registry.getAll();
    for (const agentLoop of agentLoops) {
      const triggers = agentLoop.config.triggers || [];
      const trigger = triggers.find((t: AgentTrigger) => t.id === id);
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
  protected async getAllResources(): Promise<AgentTrigger[]> {
    const agentLoops = this.registry.getAll();
    const allTriggers: AgentTrigger[] = [];

    for (const agentLoop of agentLoops) {
      const triggers = agentLoop.config.triggers || [];
      allTriggers.push(...triggers);
    }

    return allTriggers;
  }

  /**
   * Apply filter criteria
   */
  protected override applyFilter(triggers: AgentTrigger[], filter: AgentTriggerFilter): AgentTrigger[] {
    return triggers.filter(trigger => {
      if (filter.ids && !filter.ids.includes(trigger.id)) {
        return false;
      }
      if (filter.name && !trigger.name?.toLowerCase().includes(filter.name.toLowerCase())) {
        return false;
      }
      if (filter.enabled !== undefined) {
        if (trigger.enabled !== filter.enabled) {
          return false;
        }
      }
      if (filter.triggerType && trigger.condition?.eventType !== filter.triggerType) {
        return false;
      }
      return true;
    });
  }

  // ============================================================================
  // Trigger Specific Methods
  // ============================================================================

  /**
   * Get all triggers for an agent loop
   * @param agentLoopId: Agent Loop ID
   * @param filter: Filter criteria
   * @returns: Array of triggers
   */
  async getAgentLoopTriggers(
    agentLoopId: ID,
    filter?: AgentTriggerFilter,
  ): Promise<AgentTrigger[]> {
    const agentLoop = await this.registry.get(agentLoopId);
    if (!agentLoop) {
      throw new NotFoundError(
        `Agent Loop not found: ${agentLoopId}`,
        "agent_loop",
        agentLoopId,
      );
    }

    let triggers = agentLoop.config.triggers || [];

    // Apply filter criteria
    if (filter) {
      triggers = this.applyFilter(triggers, filter);
    }

    return triggers;
  }

  /**
   * Get the specified trigger for an agent loop
   * @param agentLoopId: Agent Loop ID
   * @param triggerId: Trigger ID
   * @returns: Trigger object
   */
  async getAgentLoopTrigger(agentLoopId: ID, triggerId: string): Promise<AgentTrigger> {
    const agentLoop = await this.registry.get(agentLoopId);
    if (!agentLoop) {
      throw new NotFoundError(
        `Agent Loop not found: ${agentLoopId}`,
        "agent_loop",
        agentLoopId,
      );
    }

    const trigger = (agentLoop.config.triggers || []).find((t: any) => t.id === triggerId);

    if (!trigger) {
      throw new NotFoundError(`Trigger not found: ${triggerId}`, "trigger", triggerId);
    }

    return trigger;
  }

  /**
   * Enable trigger
   * @param agentLoopId Agent Loop ID
   * @param triggerId Trigger ID
   */
  async enableTrigger(agentLoopId: ID, triggerId: string): Promise<void> {
    const trigger = await this.getAgentLoopTrigger(agentLoopId, triggerId);
    trigger.enabled = true;
  }

  /**
   * Disable the trigger
   * @param agentLoopId Agent Loop ID
   * @param triggerId Trigger ID
   */
  async disableTrigger(agentLoopId: ID, triggerId: string): Promise<void> {
    const trigger = await this.getAgentLoopTrigger(agentLoopId, triggerId);
    trigger.enabled = false;
  }

  /**
   * Check if the trigger is enabled.
   * @param agentLoopId Agent Loop ID
   * @param triggerId Trigger ID
   * @returns Whether it is enabled
   */
  async isTriggerEnabled(agentLoopId: ID, triggerId: string): Promise<boolean> {
    const trigger = await this.getAgentLoopTrigger(agentLoopId, triggerId);
    return trigger.enabled === true;
  }

  /**
   * Retrieve trigger statistics
   * @param agentLoopId Agent Loop ID
   * @returns Statistical information
   */
  async getTriggerStatistics(agentLoopId: ID): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byType: Record<string, number>;
  }> {
    const triggers = await this.getAgentLoopTriggers(agentLoopId);

    const stats = {
      total: triggers.length,
      enabled: 0,
      disabled: 0,
      byType: {} as Record<string, number>,
    };

    for (const trigger of triggers) {
      if (trigger.enabled === true) {
        stats.enabled++;
      } else {
        stats.disabled++;
      }

      const type = trigger.condition?.eventType || "unknown";
      stats.byType[type] = (stats.byType[type] || 0) + 1;
    }

    return stats;
  }

  /**
   * Get trigger statistics for all agent loops
   * @returns Global statistics information
   */
  async getGlobalTriggerStatistics(): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byAgentLoop: Record<ID, number>;
    byType: Record<string, number>;
  }> {
    const agentLoops = this.registry.getAll();
    const stats = {
      total: 0,
      enabled: 0,
      disabled: 0,
      byAgentLoop: {} as Record<ID, number>,
      byType: {} as Record<string, number>,
    };

    for (const agentLoop of agentLoops) {
      const triggers = agentLoop.config.triggers || [];

      stats.byAgentLoop[agentLoop.id] = triggers.length;
      stats.total += triggers.length;

      for (const trigger of triggers) {
        if (trigger.enabled === true) {
          stats.enabled++;
        } else {
          stats.disabled++;
        }

        const type = trigger.condition?.eventType || "unknown";
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
  async searchTriggers(query: string): Promise<AgentTrigger[]> {
    const allTriggers = await this.getAllResources();
    return allTriggers.filter(
      trigger =>
        trigger.name?.toLowerCase().includes(query.toLowerCase()) ||
        trigger.id.toLowerCase().includes(query.toLowerCase()),
    );
  }

  /**
   * Export agent loop triggers
   * @param agentLoopId: Agent Loop ID
   * @returns: JSON string
   */
  async exportAgentLoopTriggers(agentLoopId: ID): Promise<string> {
    const triggers = await this.getAgentLoopTriggers(agentLoopId);
    return JSON.stringify(triggers, null, 2);
  }

  // ============================================================================
  // Auxiliary method
  // ============================================================================

  /**
   * Get the underlying AgentLoopRegistry instance
   * @returns AgentLoopRegistry instance
   */
  getRegistry(): AgentLoopRegistry {
    return this.registry;
  }
}
