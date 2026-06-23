/**
 * AgentVariableResourceAPI - Agent Variable Resource Management API
 * Manages context variables and state within agent loop executions
 *
 * Unlike Workflow which has a dedicated VariableStateManager, Agent manages
 * variables through the execution context and message history. This API provides
 * a unified interface for accessing and managing agent variables.
 */

import { now } from "@wf-agent/common-utils";
import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { AgentLoopRegistry } from "../../../agent/stores/agent-loop-registry.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { NotFoundError, SDKError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Variable Filter for Agent Variables
 */
export interface AgentVariableFilter {
  /** Variable names (fuzzy matching is supported) */
  name?: string;
  /** Source of the variable (context, message, config, etc.) */
  source?: "context" | "message" | "config" | "state";
  /** Execution ID */
  executionId?: string;
}

/**
 * Variable update options
 */
export interface AgentVariableUpdateOptions {
  /** Whether to overwrite an existing variable */
  overwrite?: boolean;
  /** Variable source for tracking origin */
  source?: string;
}

/**
 * Agent Variable Definition
 */
export interface AgentVariableDefinition {
  /** Variable name */
  name: string;
  /** Variable type */
  type: string;
  /** Variable description */
  description?: string;
  /** Current value */
  value?: unknown;
  /** Source (where the variable comes from) */
  source: "context" | "message" | "config" | "state";
  /** Last updated timestamp */
  lastUpdated?: number;
}

/**
 * Agent Context Variable Record
 */
export interface AgentContextVariable {
  name: string;
  value: unknown;
  source: string;
  timestamp: number;
}

/**
 * AgentVariableResourceAPI - Agent Variable Resource Management API
 */
export class AgentVariableResourceAPI extends QueryableResourceAPI<unknown, string, AgentVariableFilter> {
  private registry: AgentLoopRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getAgentLoopRegistry();
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get the value of a single variable
   * @param id The variable name (format: executionId:variableName)
   * @returns The variable value; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<unknown | null> {
    const [executionId, variableName] = this.parseVariableId(id);
    await this.getAgentLoopEntity(executionId);

    // Try to get variable from context
    const variables = await this.getExecutionVariables(executionId);
    return variables[variableName] ?? null;
  }

  /**
   * Get all variable values
   * @returns Record of variable values (empty array - use specific methods instead)
   */
  protected async getAllResources(): Promise<unknown[]> {
    return [];
  }

  // ============================================================================
  // Agent-specific methods
  // ============================================================================

  /**
   * Get all variables in an agent loop execution context
   * @param executionId Execution ID
   * @returns Record of variables extracted from execution context
   */
  async getExecutionVariables(executionId: string): Promise<Record<string, unknown>> {
    const entity = await this.getAgentLoopEntity(executionId);

    // Collect variables from execution state
    const variables: Record<string, unknown> = {};

    // From agent config ID
    if (entity.config.agentConfigId) {
      variables["agentConfigId"] = entity.config.agentConfigId;
    }

    // From profile ID
    if (entity.config.profileId) {
      variables["profileId"] = entity.config.profileId;
    }

    // From initial messages (if any)
    if (entity.config.initialMessages && entity.config.initialMessages.length > 0) {
      variables["initialMessages"] = entity.config.initialMessages;
    }

    // From system prompt variables
    if (entity.config.systemPromptTemplateVariables) {
      Object.assign(variables, entity.config.systemPromptTemplateVariables);
    }

    // From state context
    const stateVars = await this.extractStateVariables(entity);
    Object.assign(variables, stateVars);

    return variables;
  }

  /**
   * Get a specific variable value from execution
   * @param executionId Execution ID
   * @param name Variable name
   * @returns Variable value
   */
  async getExecutionVariable(executionId: string, name: string): Promise<unknown> {
    const variables = await this.getExecutionVariables(executionId);
    const value = variables[name];

    if (value === undefined) {
      throw new NotFoundError(`Variable not found: ${name}`, "Variable", name);
    }

    return value;
  }

  /**
   * Check if a variable exists in execution
   * @param executionId Execution ID
   * @param name Variable name
   * @returns Whether the variable exists
   */
  async hasExecutionVariable(executionId: string, name: string): Promise<boolean> {
    const variables = await this.getExecutionVariables(executionId);
    return name in variables;
  }

  /**
   * Get variable definitions for an execution
   * @param executionId Execution ID
   * @returns Record of variable definitions
   */
  async getExecutionVariableDefinitions(executionId: string): Promise<Record<string, AgentVariableDefinition>> {
    const entity = await this.getAgentLoopEntity(executionId);
    const definitions: Record<string, AgentVariableDefinition> = {};

    // Extract from config IDs
    if (entity.config.agentConfigId) {
      definitions["agentConfigId"] = {
        name: "agentConfigId",
        type: typeof entity.config.agentConfigId,
        value: entity.config.agentConfigId,
        source: "config",
        lastUpdated: entity.state.startTime ?? now(),
      };
    }

    if (entity.config.profileId) {
      definitions["profileId"] = {
        name: "profileId",
        type: typeof entity.config.profileId,
        value: entity.config.profileId,
        source: "config",
        lastUpdated: entity.state.startTime ?? now(),
      };
    }

    // Extract from system prompt variables
    if (entity.config.systemPromptTemplateVariables) {
      for (const [name, value] of Object.entries(entity.config.systemPromptTemplateVariables)) {
        definitions[`systemPrompt_${name}`] = {
          name: `systemPrompt_${name}`,
          type: typeof value,
          value,
          source: "config",
          lastUpdated: now(),
        };
      }
    }

    // Extract from iteration history
    const iterationVars = await this.extractIterationVariables(entity);
    for (const [name, value] of Object.entries(iterationVars)) {
      definitions[name] = {
        name,
        type: typeof value,
        value,
        source: "state",
        lastUpdated: now(),
      };
    }

    return definitions;
  }

  /**
   * Get variable statistics for all executions
   * @returns Statistical information
   */
  async getVariableStatistics(): Promise<{
    totalExecutions: number;
    totalVariables: number;
    byExecution: Record<string, number>;
    bySource: Record<string, number>;
  }> {
    const entities = this.registry.getAll();
    const stats = {
      totalExecutions: entities.length,
      totalVariables: 0,
      byExecution: {} as Record<string, number>,
      bySource: {
        config: 0,
        state: 0,
        message: 0,
        context: 0,
      },
    };

    for (const entity of entities) {
      const variables = await this.getExecutionVariables(entity.id);
      const varCount = Object.keys(variables).length;

      stats.byExecution[entity.id] = varCount;
      stats.totalVariables += varCount;
      stats.bySource.config += varCount; // Simplified: all variables counted as config
    }

    return stats;
  }

  /**
   * Search for variables
   * @param executionId Execution ID
   * @param query Search keyword
   * @returns Array of matching variable names
   */
  async searchVariables(executionId: string, query: string): Promise<AgentVariableDefinition[]> {
    const definitions = await this.getExecutionVariableDefinitions(executionId);

    return Object.values(definitions).filter(def =>
      def.name.toLowerCase().includes(query.toLowerCase()),
    );
  }

  /**
   * Export execution variables
   * @param executionId Execution ID
   * @returns JSON string
   */
  async exportExecutionVariables(executionId: string): Promise<string> {
    const variables = await this.getExecutionVariables(executionId);
    return JSON.stringify(variables, null, 2);
  }

  /**
   * Get context snapshot at specific iteration
   * @param executionId Execution ID
   * @param iterationNumber Iteration number
   * @returns Variables snapshot at that iteration
   */
  async getVariablesAtIteration(executionId: string, iterationNumber: number): Promise<Record<string, unknown>> {
    const entity = await this.getAgentLoopEntity(executionId);

    // Get iteration history
    const iterationHistory = entity.state.iterationHistory;
    if (iterationNumber < 0 || iterationNumber >= iterationHistory.length) {
      throw new NotFoundError(
        `Iteration ${iterationNumber} not found`,
        "Iteration",
        iterationNumber.toString(),
      );
    }

    // For now, return current variables
    // In a full implementation, this would track variable changes per iteration
    return this.getExecutionVariables(executionId);
  }

  // ============================================================================
  // Helper methods
  // ============================================================================

  /**
   * Parse variable ID in format "executionId:variableName"
   */
  private parseVariableId(id: string): [string, string] {
    const [executionId, ...rest] = id.split(":");
    if (!executionId || rest.length === 0) {
      throw new SDKError("Invalid variable ID format. Expected 'executionId:variableName'");
    }
    return [executionId, rest.join(":")];
  }

  /**
   * Get agent loop entity
   */
  private async getAgentLoopEntity(executionId: string): Promise<AgentLoopEntity> {
    const entity = await this.registry.get(executionId);
    if (!entity) {
      throw new NotFoundError(`Agent loop execution not found: ${executionId}`, "AgentLoop", executionId);
    }
    return entity;
  }

  /**
   * Extract variables from agent state
   */
  private async extractStateVariables(entity: AgentLoopEntity): Promise<Record<string, unknown>> {
    const variables: Record<string, unknown> = {};

    // Extract from state information
    variables["currentIteration"] = entity.state.currentIteration;
    variables["status"] = entity.state.status;
    variables["toolCallCount"] = entity.state.toolCallCount;
    variables["startTime"] = entity.state.startTime;
    variables["endTime"] = entity.state.endTime;

    return variables;
  }

  /**
   * Extract variables from iteration history
   */
  private async extractIterationVariables(entity: AgentLoopEntity): Promise<Record<string, unknown>> {
    const variables: Record<string, unknown> = {};

    const history = entity.state.iterationHistory;
    if (history.length > 0) {
      // Get the latest iteration
      const lastIteration = history[history.length - 1];
      if (lastIteration) {
        variables["lastIterationOutput"] = lastIteration.responseContent;
        variables["lastIterationToolCalls"] = lastIteration.toolCalls?.length ?? 0;
      }
    }

    return variables;
  }
}
