/**
 * AgentLoopVariableResourceAPI - Agent Loop Variable Resource Management API
 * Inherits ReadonlyResourceAPI, provides read-only operations
 *
 * Responsibilities:
 * - Encapsulates VariableState, provides variable state management functionality
 * - Supports variable query, search, statistics and other functionalities
 */

import { ReadonlyResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { ID } from "@wf-agent/types";
import type { AgentLoopRegistry } from "../../../agent/stores/agent-loop-registry.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";

/**
 * Variable Filter
 */
export interface AgentLoopVariableFilter {
  /** Agent Loop ID */
  agentLoopId?: ID;
  /** Variable name prefix */
  namePrefix?: string;
}

/**
 * Variable definition information
 */
export interface VariableDefinition {
  /** Variable name */
  name: string;
  /** Variable Types */
  type: string;
  /** Variable values */
  value: unknown;
}

/**
 * AgentLoopVariableResourceAPI - Agent Loop Variable Resource Management API
 */
export class AgentLoopVariableResourceAPI extends ReadonlyResourceAPI<
  unknown,
  string,
  AgentLoopVariableFilter
> {
  private registry: AgentLoopRegistry;

  constructor() {
    super();
    const container = getContainer();
    this.registry = container.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry;
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get the value of a single variable
   * @param id Variable ID (format: agentLoopId:variableName)
   * @returns The value of the variable; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<unknown | null> {
    const [agentLoopId, variableName] = id.split(":");
    if (!agentLoopId || !variableName) {
      return null;
    }

    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return null;
    }

    const value = entity.getVariable(variableName);
    return value !== undefined ? value : null;
  }

  /**
   * Get all variable values
   * @returns Array of variable values
   */
  protected async getAllResources(): Promise<unknown[]> {
    // The variable is not suitable for returning an array, so an empty array is returned instead. A specific method is used to obtain the variable.
    return [];
  }

  // ============================================================================
  // Variable-specific methods
  // ============================================================================

  /**
   * Get all variables of the Agent Loop
   * @param agentLoopId Agent Loop ID
   * @returns Record of variables
   */
  async getAgentLoopVariables(agentLoopId: ID): Promise<Record<string, unknown>> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return {};
    }

    return entity.getAllVariables();
  }

  /**
   * Get the specified variable from the Agent Loop
   * @param agentLoopId Agent Loop ID
   * @param name Variable name
   * @returns Variable value
   */
  async getAgentLoopVariable(agentLoopId: ID, name: string): Promise<unknown> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      throw new Error(`Agent Loop not found: ${agentLoopId}`);
    }

    const value = entity.getVariable(name);
    if (value === undefined) {
      throw new Error(`Variable not found: ${name}`);
    }

    return value;
  }

  /**
   * Check if the specified variable exists in the Agent Loop
   * @param agentLoopId Agent Loop ID
   * @param name Variable name
   * @returns Whether the variable exists
   */
  async hasAgentLoopVariable(agentLoopId: ID, name: string): Promise<boolean> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return false;
    }

    const value = entity.getVariable(name);
    return value !== undefined;
  }

  /**
   * Get the list of variable definitions
   * @param agentLoopId Agent Loop ID
   * @returns Array of variable definitions
   */
  async getVariableDefinitions(agentLoopId: ID): Promise<VariableDefinition[]> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    const variables = entity.getAllVariables();
    const definitions: VariableDefinition[] = [];

    for (const [name, value] of Object.entries(variables)) {
      definitions.push({
        name,
        type: typeof value,
        value,
      });
    }

    return definitions;
  }

  /**
   * Get statistics for all variables in the Agent Loop
   * @returns Statistical information
   */
  async getVariableStatistics(): Promise<{
    totalAgentLoops: number;
    totalVariables: number;
    byAgentLoop: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const entities = this.registry.getAll();
    const stats = {
      totalAgentLoops: entities.length,
      totalVariables: 0,
      byAgentLoop: {} as Record<string, number>,
      byType: {} as Record<string, number>,
    };

    for (const entity of entities) {
      const agentLoopId = entity.id;
      const variables = entity.getAllVariables();

      stats.byAgentLoop[agentLoopId] = Object.keys(variables).length;
      stats.totalVariables += Object.keys(variables).length;

      // Statistical variable types
      for (const [, value] of Object.entries(variables)) {
        const type = typeof value;
        stats.byType[type] = (stats.byType[type] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Search for variables
   * @param agentLoopId Agent Loop ID
   * @param query Search keyword
   * @returns Array of matching variable names
   */
  async searchVariables(agentLoopId: ID, query: string): Promise<string[]> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return [];
    }

    const variables = entity.getAllVariables();
    return Object.keys(variables).filter(name => name.toLowerCase().includes(query.toLowerCase()));
  }

  /**
   * Export the Agent Loop variable
   * @param agentLoopId Agent Loop ID
   * @returns JSON string
   */
  async exportAgentLoopVariables(agentLoopId: ID): Promise<string> {
    const variables = await this.getAgentLoopVariables(agentLoopId);
    return JSON.stringify(variables, null, 2);
  }

  /**
   * Get the number of variables
   * @param agentLoopId Agent Loop ID
   * @returns Number of variables
   */
  async getVariableCount(agentLoopId: ID): Promise<number> {
    const entity = this.registry.get(agentLoopId);
    if (!entity) {
      return 0;
    }

    return Object.keys(entity.getAllVariables()).length;
  }

  /**
   * Get the underlying AgentLoopRegistry instance
   * @returns AgentLoopRegistry instance
   */
  getRegistry(): AgentLoopRegistry {
    return this.registry;
  }
}
