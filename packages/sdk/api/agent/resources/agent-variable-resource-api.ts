/**
 * AgentVariableResourceAPI - Agent Variable Resource Management API
 * Provides APIs for managing variables in agent loop executions.
 * Extends the shared BaseVariableResourceAPI with agent-specific implementation.
 */

import {
  BaseVariableResourceAPI,
  type BaseVariableFilter,
  type VariableStatistics,
} from "../../shared/resources/variable-base.js";
import { now } from "@wf-agent/common-utils";
import type { AgentLoopRegistry } from "../../../agent/registry/agent-loop-registry.js";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { NotFoundError, SDKError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { success, failure } from "../../shared/types/execution-result.js";
import type { ExecutionResult } from "../../shared/types/execution-result.js";

/**
 * Agent Variable Filter
 */
export interface AgentVariableFilter extends BaseVariableFilter {
  /** Filter by source */
  source?: string;
}

/**
 * Agent Variable Update Options
 */
export interface AgentVariableUpdateOptions {
  /** Whether to overwrite existing variables */
  overwrite?: boolean;
  /** Source of the update */
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
  /** Variable value */
  value: unknown;
  /** Variable source */
  source: string;
  /** Last updated timestamp */
  lastUpdated: number;
  /** Whether the variable is required */
  required?: boolean;
}

/**
 * Agent Context Variable
 */
export interface AgentContextVariable {
  /** Variable name */
  name: string;
  /** Variable value */
  value: unknown;
  /** Variable scope */
  scope: "execution" | "iteration" | "global";
}

/**
 * AgentVariableResourceAPI - Agent Variable Resource Management API
 */
export class AgentVariableResourceAPI extends BaseVariableResourceAPI<AgentVariableFilter> {
  private registry: AgentLoopRegistry;
  private _customVariables: Map<string, Record<string, unknown>> = new Map();

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getAgentLoopRegistry();
  }

  // ============================================================================
  // Implement BaseVariableResourceAPI abstract methods
  // ============================================================================

  /**
   * Get all variables in an agent loop execution context
   */
  protected async getEntityVariables(executionId: string): Promise<Record<string, unknown>> {
    const entity = await this.getAgentLoopEntity(executionId);

    const variables: Record<string, unknown> = {};

    // From agent config ID
    if (entity.config.agentConfigId) {
      variables["agentConfigId"] = entity.config.agentConfigId;
    }

    // From profile ID
    if (entity.config.profileId) {
      variables["profileId"] = entity.config.profileId;
    }

    // From initial messages
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

    // From custom variables store
    const customVars = this._customVariables.get(executionId);
    if (customVars) {
      Object.assign(variables, customVars);
    }

    return variables;
  }

  /**
   * Get a specific variable value
   */
  protected async getEntityVariable(executionId: string, name: string): Promise<unknown> {
    const variables = await this.getEntityVariables(executionId);
    const value = variables[name];

    if (value === undefined) {
      throw new NotFoundError(`Variable not found: ${name}`, "Variable", name);
    }

    return value;
  }

  /**
   * Check if a variable exists
   */
  protected async hasEntityVariable(executionId: string, name: string): Promise<boolean> {
    const variables = await this.getEntityVariables(executionId);
    return name in variables;
  }

  /**
   * Set a variable value
   */
  protected async setEntityVariable(
    executionId: string,
    name: string,
    value: unknown,
  ): Promise<ExecutionResult<void>> {
    try {
      await this.getAgentLoopEntity(executionId);

      let vars = this._customVariables.get(executionId);
      if (!vars) {
        vars = {};
        this._customVariables.set(executionId, vars);
      }
      vars[name] = value;
      return success(undefined, 0);
    } catch (error) {
      const sdkError =
        error instanceof SDKError
          ? error
          : error instanceof Error
            ? new SDKError(error.message, "error", undefined, error)
            : new SDKError(String(error), "error");
      return failure(sdkError, 0);
    }
  }

  /**
   * Delete a variable
   */
  protected async deleteEntityVariable(
    executionId: string,
    name: string,
  ): Promise<ExecutionResult<void>> {
    try {
      await this.getAgentLoopEntity(executionId);

      const vars = this._customVariables.get(executionId);
      if (vars && name in vars) {
        delete vars[name];
      }
      return success(undefined, 0);
    } catch (error) {
      const sdkError =
        error instanceof SDKError
          ? error
          : error instanceof Error
            ? new SDKError(error.message, "error", undefined, error)
            : new SDKError(String(error), "error");
      return failure(sdkError, 0);
    }
  }

  /**
   * Get variable definitions for an execution
   */
  protected async getEntityVariableDefinitions(
    executionId: string,
  ): Promise<Record<string, unknown>> {
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

    return definitions as unknown as Record<string, unknown>;
  }

  /**
   * Parse variable ID
   */
  protected parseVariableId(id: string): [string, string] {
    const [executionId, ...rest] = id.split(":");
    if (!executionId || rest.length === 0) {
      throw new SDKError("Invalid variable ID format. Expected 'executionId:variableName'");
    }
    return [executionId, rest.join(":")];
  }

  /**
   * Get variable statistics
   */
  protected async getVariableStatistics(): Promise<VariableStatistics> {
    const entities = this.registry.getAll();
    const stats: VariableStatistics = {
      totalExecutions: entities.length,
      totalVariables: 0,
      byExecution: {},
      byType: {},
      bySource: {},
    };

    for (const entity of entities) {
      const variables = await this.getEntityVariables(entity.id);
      const varCount = Object.keys(variables).length;

      stats.byExecution[entity.id] = varCount;
      stats.totalVariables += varCount;
      stats.bySource["config"] = (stats.bySource["config"] || 0) + varCount;
    }

    return stats;
  }

  // ============================================================================
  // Agent-specific methods
  // ============================================================================

  /**
   * Get all variables in an agent loop execution context
   */
  async getExecutionVariables(executionId: string): Promise<Record<string, unknown>> {
    return this.getEntityVariables(executionId);
  }

  /**
   * Get a specific variable value from execution
   */
  async getExecutionVariable(executionId: string, name: string): Promise<unknown> {
    return this.getEntityVariable(executionId, name);
  }

  /**
   * Check if a variable exists in execution
   */
  async hasExecutionVariable(executionId: string, name: string): Promise<boolean> {
    return this.hasEntityVariable(executionId, name);
  }

  /**
   * Get variable definitions for an execution
   */
  async getExecutionVariableDefinitions(
    executionId: string,
  ): Promise<Record<string, AgentVariableDefinition>> {
    const definitions = await this.getEntityVariableDefinitions(executionId);
    return definitions as unknown as Record<string, AgentVariableDefinition>;
  }

  /**
   * Get variable statistics (public API - renamed to avoid conflict with base class)
   */
  async getAgentVariableStatistics(): Promise<{
    totalExecutions: number;
    totalVariables: number;
    byExecution: Record<string, number>;
    bySource: Record<string, number>;
  }> {
    const stats = await this.getVariableStatistics();
    return {
      totalExecutions: stats.totalExecutions,
      totalVariables: stats.totalVariables,
      byExecution: stats.byExecution,
      bySource: stats.bySource,
    };
  }

  /**
   * Search for variables
   */
  async searchVariables(executionId: string, query: string): Promise<AgentVariableDefinition[]> {
    const results = await this.searchEntityVariables(executionId, query);
    return results as AgentVariableDefinition[];
  }

  /**
   * Export execution variables
   */
  async exportExecutionVariables(executionId: string): Promise<string> {
    return this.exportEntityVariables(executionId);
  }

  /**
   * Get context snapshot at specific iteration
   */
  async getVariablesAtIteration(
    executionId: string,
    iterationNumber: number,
  ): Promise<Record<string, unknown>> {
    const entity = await this.getAgentLoopEntity(executionId);

    const iterationHistory = entity.state.iterationHistory;
    if (iterationNumber < 0 || iterationNumber >= iterationHistory.length) {
      throw new NotFoundError(
        `Iteration ${iterationNumber} not found`,
        "Iteration",
        iterationNumber.toString(),
      );
    }

    return this.getEntityVariables(executionId);
  }

  /**
   * Set a variable value for an agent execution
   */
  async setExecutionVariable(
    executionId: string,
    name: string,
    value: unknown,
  ): Promise<ExecutionResult<void>> {
    return this.setEntityVariable(executionId, name, value);
  }

  /**
   * Delete a variable from an agent execution
   */
  async deleteExecutionVariable(
    executionId: string,
    name: string,
  ): Promise<ExecutionResult<void>> {
    return this.deleteEntityVariable(executionId, name);
  }

  // ============================================================================
  // Helper methods
  // ============================================================================

  private async getAgentLoopEntity(executionId: string): Promise<AgentLoopEntity> {
    const entity = await this.registry.get(executionId);
    if (!entity) {
      throw new NotFoundError(
        `Agent loop execution not found: ${executionId}`,
        "AgentLoop",
        executionId,
      );
    }
    return entity;
  }

  /**
   * Get variable change history for an agent execution
   * Tracks how variables have changed over time across iterations.
   * @param executionId Execution ID
   * @param variableName Optional variable name to filter history
   * @returns Array of variable history entries
   */
  async getVariableHistory(
    executionId: string,
    variableName?: string,
  ): Promise<
    Array<{
      name: string;
      previousValue?: unknown;
      newValue: unknown;
      timestamp: number;
      iteration: number;
      source?: string;
    }>
  > {
    const entity = await this.getAgentLoopEntity(executionId);
    const history: Array<{
      name: string;
      previousValue?: unknown;
      newValue: unknown;
      timestamp: number;
      iteration: number;
      source?: string;
    }> = [];

    // Track variable changes across iterations
    const previousVars: Record<string, unknown> = {};

    for (const record of entity.state.iterationHistory) {
      const currentVars: Record<string, unknown> = {
        ...previousVars,
      };

      // Record config changes
      if (record.responseContent) {
        currentVars["lastResponse"] = record.responseContent;
      }

      // Track tool call results
      for (const toolCall of record.toolCalls) {
        if (toolCall.result !== undefined) {
          currentVars[`tool_${toolCall.name}_result`] = toolCall.result;
        }
      }

      // Detect changes
      for (const [name, value] of Object.entries(currentVars)) {
        const prevValue = previousVars[name];
        if (prevValue !== value) {
          history.push({
            name,
            previousValue: prevValue,
            newValue: value,
            timestamp: record.startTime,
            iteration: record.iteration,
            source: "iteration",
          });
        }
      }

      Object.assign(previousVars, currentVars);
    }

    // Filter by variable name if specified
    if (variableName) {
      return history.filter((h) => h.name === variableName);
    }

    return history;
  }

  /**
   * Get variable scopes for an agent execution
   * Returns the available scopes and their variables.
   * @param executionId Execution ID
   * @returns Scopes configuration
   */
  async getVariableScopes(
    executionId: string,
  ): Promise<{
    scopes: Array<{
      name: string;
      description: string;
      variables: string[];
      isMutable: boolean;
    }>;
  }> {
    const variables = await this.getEntityVariables(executionId);

    return {
      scopes: [
        {
          name: "config",
          description: "Variables from agent loop configuration",
          variables: Object.keys(variables).filter((k) =>
            ["agentConfigId", "profileId", "initialMessages"].includes(k),
          ),
          isMutable: false,
        },
        {
          name: "state",
          description: "Variables from execution state",
          variables: Object.keys(variables).filter((k) =>
            ["currentIteration", "status", "toolCallCount", "startTime", "endTime"].includes(k),
          ),
          isMutable: false,
        },
        {
          name: "custom",
          description: "Custom variables set during execution",
          variables: Object.keys(this._customVariables.get(executionId) ?? {}),
          isMutable: true,
        },
        {
          name: "iteration",
          description: "Variables from iteration outputs",
          variables: Object.keys(variables).filter(
            (k) => k.startsWith("tool_") || k === "lastResponse",
          ),
          isMutable: true,
        },
      ],
    };
  }

  private async extractStateVariables(entity: AgentLoopEntity): Promise<Record<string, unknown>> {
    const variables: Record<string, unknown> = {};
    variables["currentIteration"] = entity.state.currentIteration;
    variables["status"] = entity.state.status;
    variables["toolCallCount"] = entity.state.toolCallCount;
    variables["startTime"] = entity.state.startTime;
    variables["endTime"] = entity.state.endTime;
    return variables;
  }

  private async extractIterationVariables(
    entity: AgentLoopEntity,
  ): Promise<Record<string, unknown>> {
    const variables: Record<string, unknown> = {};
    const history = entity.state.iterationHistory;
    if (history.length > 0) {
      const lastIteration = history[history.length - 1];
      if (lastIteration) {
        variables["lastIterationOutput"] = lastIteration.responseContent;
        variables["lastIterationToolCalls"] = lastIteration.toolCalls?.length ?? 0;
      }
    }
    return variables;
  }
}