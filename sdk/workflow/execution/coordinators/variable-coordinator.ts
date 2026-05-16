/**
 * VariableCoordinator - Variable Coordinator
 * Responsible for the coordination logic of variables, including validation, initialization on demand, event triggering, etc.
 *
 * Core Responsibilities:
 * 1. Coordinate variable query and update operations
 * 2. Handle variable validation logic
 * 3. Initialize variables as needed
 * 4. Trigger variable change events
 *
 * Design Principles:
 * - Stateless design: Does not maintain any mutable state
 * - Coordination logic: Encapsulates the coordination logic for variable operations
 * - Dependency injection: Receives managers for dependencies through method parameters
 * - Delegation pattern: Uses the VariableManager for atomic state operations
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { VariableScope, VariableDefinition } from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { RuntimeValidationError } from "@wf-agent/types";
import { VariableManager } from "../../state-managers/variable-manager.js";
import { VariableAccessor } from "../utils/variable-accessor.js";
import { emit } from "../../../core/utils/event/event-emitter.js";
import { buildVariableChangedEvent } from "../utils/event/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "VariableCoordinator" });

/**
 * VariableCoordinator - Variable Coordinator
 *
 * Responsibilities:
 * - Coordinate variable query and update operations
 * - Handle variable validation logic
 * - Initialize variables as needed
 * - Trigger variable change events
 *
 * Design Principles:
 * - Stateless design: Does not maintain any mutable state
 * - Coordination logic: Encapsulates the logic for coordinating variable operations
 * - Dependency injection: Receives managers for dependencies through method parameters
 * - Delegation pattern: Uses the VariableManager to perform atomic state operations
 * 
 * IMPORTANT: This coordinator is stateless and receives VariableManager as a parameter
 * in each method call. This ensures it always operates on the correct instance.
 */
export class VariableCoordinator {
  constructor(
    private eventManager?: EventRegistry,
  ) {}

  /**
   * Initialize variables from VariableDefinition array
   * @param manager The VariableManager instance to initialize
   * @param variableDefinitions Array of variable definitions
   */
  initializeFromDefinitions(manager: VariableManager, variableDefinitions: VariableDefinition[]): void {
    manager.initializeFromDefinitions(variableDefinitions);
  }

  /**
   * Retrieve the value of a variable
   * Delegates to VariableManager.getVariable() which handles all scope priority logic
   * 
   * Priority: loop > subgraph > execution > global
   * 
   * @param manager The VariableManager instance to use
   * @param _executionEntity WorkflowExecutionEntity instance (kept for API compatibility)
   * @param name Variable name
   * @returns Variable value or undefined if not found
   */
  getVariable(manager: VariableManager, _executionEntity: WorkflowExecutionEntity, name: string): unknown {
    // Simple delegation to VariableManager
    // All scope priority logic is centralized in VariableManager.getVariable()
    return manager.getVariable(name);
  }

  /**
   * Update the value of a defined variable
   * @param manager The VariableManager instance to use
   * @param executionEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @param value New variable value
   * @param explicitScope Explicitly specified scope (optional)
   */
  async updateVariable(
    manager: VariableManager,
    executionEntity: WorkflowExecutionEntity,
    name: string,
    value: unknown,
    explicitScope?: VariableScope,
  ): Promise<void> {
    const variableDef = manager.getVariableDefinition(name);

    if (!variableDef) {
      throw new RuntimeValidationError(
        `Variable '${name}' is not defined in workflow. Variables must be defined in WorkflowTemplate.`,
        {
          operation: "setVariable",
          field: "variableName",
          value: name,
          context: { executionId: executionEntity.id, workflowId: executionEntity.getWorkflowId() },
        },
      );
    }

    if (variableDef.readonly) {
      throw new RuntimeValidationError(`Variable '${name}' is readonly and cannot be modified`, {
        operation: "setVariable",
        field: "variableName",
        value: name,
        context: { executionId: executionEntity.id, workflowId: executionEntity.getWorkflowId() },
      });
    }

    if (!this.validateType(value, variableDef.type)) {
      throw new RuntimeValidationError(
        `Type mismatch for variable '${name}'. Expected ${variableDef.type}, got ${typeof value}`,
        {
          operation: "setVariable",
          field: "variableValue",
          value: value,
          context: {
            executionId: executionEntity.id,
            workflowId: executionEntity.getWorkflowId(),
            variableName: name,
            expectedType: variableDef.type,
            actualType: typeof value,
          },
        },
      );
    }

    // If an explicit scope is specified, verify it matches the definition
    if (explicitScope && explicitScope !== variableDef.scope) {
      logger.warn("Explicit scope differs from variable definition scope", {
        name,
        definitionScope: variableDef.scope,
        explicitScope,
      });
      // Use the definition's scope (more restrictive approach)
    }

    // Delegate to VariableManager
    manager.setVariable(name, value);

    // Trigger a variable change event
    await this.emitVariableChangedEvent(executionEntity, name, value, variableDef.scope);
  }

  /**
   * Check if the variable exists
   * @param manager The VariableManager instance to use
   * @param executionEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @returns Whether the variable exists
   */
  hasVariable(manager: VariableManager, executionEntity: WorkflowExecutionEntity, name: string): boolean {
    return this.getVariable(manager, executionEntity, name) !== undefined;
  }

  /**
   * Get all variables (merged by scope priority)
   * @param manager The VariableManager instance to use
   * @returns A map of all variable key-value pairs
   */
  getAllVariables(manager: VariableManager): Record<string, unknown> {
    return manager.getAllVariables();
  }

  /**
   * Get the variable within the specified scope
   * @param manager The VariableManager instance to use
   * @param scope Variable scope
   * @returns Variable key-value pair within the specified scope
   */
  getVariablesByScope(manager: VariableManager, scope: VariableScope): Record<string, unknown> {
    return manager.getVariablesByScope(scope);
  }

  /**
   * Verify variable type
   * @param value: Variable value
   * @param expectedType: Expected type
   * @returns: Whether the types match
   */
  private validateType(value: unknown, expectedType: string): boolean {
    const actualType = typeof value;

    switch (expectedType) {
      case "number":
        return actualType === "number" && !isNaN(value as number);
      case "string":
        return actualType === "string";
      case "boolean":
        return actualType === "boolean";
      case "array":
        return Array.isArray(value);
      case "object":
        return actualType === "object" && value !== null && !Array.isArray(value);
      default:
        return false;
    }
  }

  /**
   * Copy variables (for fork scenarios)
   * @param source Source VariableManager
   * @param target Target VariableManager
   */
  copyVariables(source: VariableManager, target: VariableManager): void {
    target.copyFrom(source);
  }

  /**
   * Clear all variables
   * @param manager The VariableManager instance to clear
   */
  clearVariables(manager: VariableManager): void {
    manager.cleanup();
  }

  /**
   * Create a variable accessor
   * Provide a unified interface for accessing variables, supporting nested path resolution
   * @param executionEntity: WorkflowExecution entity
   * @returns: VariableAccessor instance
   */
  createAccessor(executionEntity: WorkflowExecutionEntity): VariableAccessor {
    return new VariableAccessor(executionEntity);
  }

  /**
   * 通过路径获取变量值
   * 支持嵌套路径和命名空间
   * @param executionEntity WorkflowExecution entity
   * @param path 变量路径
   * @returns 变量值
   *
   * @example
   * // 简单变量
   * getVariableByPath(entity, 'userName')
   *
   * // 嵌套路径
   * getVariableByPath(entity, 'user.profile.name')
   *
   * // 命名空间
   * getVariableByPath(entity, 'input.userName')
   * getVariableByPath(entity, 'output.result')
   * getVariableByPath(entity, 'global.config')
   * getVariableByPath(entity, 'workflowExecution.state')
   * getVariableByPath(entity, 'subgraph.temp')
   * getVariableByPath(entity, 'loop.item')
   */
  getVariableByPath(executionEntity: WorkflowExecutionEntity, path: string): unknown {
    const accessor = this.createAccessor(executionEntity);
    return accessor.get(path);
  }

  /**
   * Trigger a variable change event
   * Note: This is for observability only. If no listeners need to act on variable changes,
   * consider removing this and just using logger.debug().
   * 
   * @param executionEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @param value New value
   * @param scope Scope of the variable
   */
  private async emitVariableChangedEvent(
    executionEntity: WorkflowExecutionEntity,
    name: string,
    value: unknown,
    scope: VariableScope,
  ): Promise<void> {
    if (!this.eventManager) {
      // No event manager available - just log for observability
      logger.debug("Variable changed (no event manager)", {
        executionId: executionEntity.id,
        workflowId: executionEntity.getWorkflowId(),
        variableName: name,
        variableScope: scope,
      });
      return;
    }

    try {
      const event = buildVariableChangedEvent({
        executionId: executionEntity.id,
        workflowId: executionEntity.getWorkflowId(),
        variableName: name,
        variableValue: value,
        variableScope: scope,
      });
      await emit(this.eventManager, event);
    } catch (error) {
      // Event emission failed - log but don't break the main flow
      // This is observability data, not critical state
      logger.warn("Failed to emit VARIABLE_CHANGED event", {
        executionId: executionEntity.id,
        workflowId: executionEntity.getWorkflowId(),
        variableName: name,
        error: getErrorOrNew(error).message,
      });
    }
  }
}
