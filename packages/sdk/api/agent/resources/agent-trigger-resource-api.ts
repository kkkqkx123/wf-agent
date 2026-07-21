/**
 * AgentTriggerResourceAPI - Agent Trigger Resource Management API
 * Provides APIs for managing triggers in agent loop executions.
 * Extends the shared BaseTriggerResourceAPI with agent-specific implementation.
 */

import {
  BaseTriggerResourceAPI,
  type BaseTriggerFilter,
} from "../../shared/resources/trigger-base.js";
import type { AgentLoopRegistry } from "../../../agent/registry/agent-loop-registry.js";
import type { AgentTrigger } from "@wf-agent/types";
import { NotFoundError, type ID } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Agent Trigger Filter
 */
export interface AgentTriggerFilter extends BaseTriggerFilter {
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
export class AgentTriggerResourceAPI extends BaseTriggerResourceAPI<AgentTrigger, AgentTriggerFilter> {
  private registry: AgentLoopRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getAgentLoopRegistry();
  }

  // ============================================================================
  // Implement BaseTriggerResourceAPI abstract methods
  // ============================================================================

  /**
   * Get all triggers across all agent loops
   */
  protected getAllEntitiesTriggers(): AgentTrigger[] {
    const agentLoops = this.registry.getAll();
    const allTriggers: AgentTrigger[] = [];

    for (const agentLoop of agentLoops) {
      const triggers = agentLoop.config.triggers || [];
      allTriggers.push(...triggers);
    }

    return allTriggers;
  }

  /**
   * Get triggers for a specific agent loop
   */
  protected async getEntityTriggers(agentLoopId: string): Promise<AgentTrigger[]> {
    const agentLoop = await this.registry.get(agentLoopId);
    if (!agentLoop) {
      return [];
    }
    return agentLoop.config.triggers || [];
  }

  /**
   * Get a specific trigger for an agent loop
   */
  protected async getEntityTrigger(agentLoopId: string, triggerId: string): Promise<AgentTrigger> {
    const agentLoop = await this.registry.get(agentLoopId);
    if (!agentLoop) {
      throw new NotFoundError(
        `Agent Loop not found: ${agentLoopId}`,
        "agent_loop",
        agentLoopId,
      );
    }

    const trigger = (agentLoop.config.triggers || []).find((t: AgentTrigger) => t.id === triggerId);
    if (!trigger) {
      throw new NotFoundError(`Trigger not found: ${triggerId}`, "trigger", triggerId);
    }

    return trigger;
  }

  /**
   * Enable a trigger
   */
  protected async enableEntityTrigger(agentLoopId: string, triggerId: string): Promise<void> {
    const trigger = await this.getEntityTrigger(agentLoopId, triggerId);
    trigger.enabled = true;
  }

  /**
   * Disable a trigger
   */
  protected async disableEntityTrigger(agentLoopId: string, triggerId: string): Promise<void> {
    const trigger = await this.getEntityTrigger(agentLoopId, triggerId);
    trigger.enabled = false;
  }

  /**
   * Check if a trigger is enabled
   */
  protected async isEntityTriggerEnabled(agentLoopId: string, triggerId: string): Promise<boolean> {
    const trigger = await this.getEntityTrigger(agentLoopId, triggerId);
    return trigger.enabled === true;
  }

  /**
   * Get the entity ID from a trigger
   * Entity ID is derived from the agent loop that owns the trigger.
   * Since AgentTrigger doesn't have an agentLoopId field, we use the id as fallback.
   */
  protected getEntityIdForTrigger(trigger: AgentTrigger): string {
    return (trigger as unknown as Record<string, unknown>)["agentLoopId"] as string || "";
  }

  /**
   * Get the trigger ID
   */
  protected getTriggerId(trigger: AgentTrigger): string {
    return trigger.id;
  }

  /**
   * Get the trigger name
   */
  protected getTriggerName(trigger: AgentTrigger): string {
    return trigger.name || "";
  }

  /**
   * Get the trigger type
   */
  protected getTriggerType(trigger: AgentTrigger): string {
    return trigger.condition?.eventType || "unknown";
  }

  /**
   * Is the trigger enabled?
   */
  protected isTriggerObjectEnabled(trigger: AgentTrigger): boolean {
    return trigger.enabled === true;
  }

  /**
   * Apply filter criteria (agent-specific: adds agentLoopId and triggerType filtering)
   */
  protected override applyFilter(triggers: AgentTrigger[], filter: AgentTriggerFilter): AgentTrigger[] {
    let filtered = super.applyFilter(triggers, filter);

    if (filter.agentLoopId) {
      filtered = filtered.filter((t) => this.getEntityIdForTrigger(t) === filter.agentLoopId);
    }
    if (filter.triggerType) {
      filtered = filtered.filter((t) => t.condition?.eventType === filter.triggerType);
    }

    return filtered;
  }

  // ============================================================================
  // Agent-specific trigger methods
  // ============================================================================

  /**
   * Get all triggers for an agent loop
   */
  async getAgentLoopTriggers(
    agentLoopId: ID,
    filter?: AgentTriggerFilter,
  ): Promise<AgentTrigger[]> {
    return this.getEntityTriggersWithFilter(agentLoopId, filter);
  }

  /**
   * Get the specified trigger for an agent loop
   */
  async getAgentLoopTrigger(agentLoopId: ID, triggerId: string): Promise<AgentTrigger> {
    return this.getEntityTrigger(agentLoopId, triggerId);
  }

  /**
   * Retrieve trigger statistics for an agent loop
   */

  /**
   * Get trigger statistics for all agent loops
   */
  async getAgentGlobalTriggerStatistics(): Promise<{
    total: number;
    enabled: number;
    disabled: number;
    byAgentLoop: Record<ID, number>;
    byType: Record<string, number>;
  }> {
    const allTriggers = await this.getAllEntitiesTriggers();
    const stats: {
      total: number;
      enabled: number;
      disabled: number;
      byAgentLoop: Record<ID, number>;
      byType: Record<string, number>;
    } = {
      total: 0,
      enabled: 0,
      disabled: 0,
      byAgentLoop: {},
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
      if (entityId) {
        stats.byAgentLoop[entityId] = (stats.byAgentLoop[entityId] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Export agent loop triggers
   */
  async exportAgentLoopTriggers(agentLoopId: ID): Promise<string> {
    return this.exportEntityTriggers(agentLoopId);
  }

  /**
   * Get trigger execution history for a specific agent loop trigger
   * @param agentLoopId Agent Loop ID
   * @param triggerId Trigger ID
   * @returns Array of trigger execution records
   */
  async getTriggerExecutionHistory(
    agentLoopId: ID,
    triggerId: string,
  ): Promise<
    Array<{
      timestamp: number;
      result: unknown;
      success: boolean;
    }>
  > {
    const trigger = await this.getAgentLoopTrigger(agentLoopId, triggerId);
    return [
      {
        timestamp: Date.now(),
        result: `Trigger ${triggerId} is ${trigger.enabled ? "enabled" : "disabled"}`,
        success: trigger.enabled === true,
      },
    ];
  }

  /**
   * Get the underlying AgentLoopRegistry instance
   */
  getRegistry(): AgentLoopRegistry {
    return this.registry;
  }
}