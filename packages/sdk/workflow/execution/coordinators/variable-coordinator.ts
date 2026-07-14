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
import type { VariableDefinition } from "@wf-agent/types";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { RuntimeValidationError } from "@wf-agent/types";
import { VariableManager } from "../utils/variable-manager.js";
import { emit } from "../../../shared/events/emit-event.js";
import { buildVariableChangedEvent } from "../../../shared/events/builders/index.js";
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
  constructor(private eventManager?: EventRegistry) {}

  /**
   * Initialize variables from VariableDefinition array
   * @param manager The VariableManager instance to initialize
   * @param variableDefinitions Array of variable definitions
   */
  initializeFromDefinitions(
    manager: VariableManager,
    variableDefinitions: VariableDefinition[],
  ): void {
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
  getVariable(
    manager: VariableManager,
    _executionEntity: WorkflowExecutionEntity,
    name: string,
  ): unknown {
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
   */
  async updateVariable(
    manager: VariableManager,
    executionEntity: WorkflowExecutionEntity,
    name: string,
    value: unknown,
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

    // Delegate to VariableManager
    manager.setVariable(name, value);

    // Trigger a variable change event
    await this.emitVariableChangedEvent(executionEntity, name, value);
  }

  /**
   * Check if the variable exists
   * @param manager The VariableManager instance to use
   * @param executionEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @returns Whether the variable exists
   */
  hasVariable(
    manager: VariableManager,
    executionEntity: WorkflowExecutionEntity,
    name: string,
  ): boolean {
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
   * Trigger a variable change event
   * Note: This is for observability only. If no listeners need to act on variable changes,
   * consider removing this and just using logger.debug().
   *
   * @param executionEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @param value New value
   */
  private async emitVariableChangedEvent(
    executionEntity: WorkflowExecutionEntity,
    name: string,
    value: unknown,
  ): Promise<void> {
    if (!this.eventManager) {
      // No event manager available - just log for observability
      logger.debug("Variable changed (no event manager)", {
        executionId: executionEntity.id,
        workflowId: executionEntity.getWorkflowId(),
        variableName: name,
      });
      return;
    }

    try {
      const event = buildVariableChangedEvent({
        executionId: executionEntity.id,
        workflowId: executionEntity.getWorkflowId(),
        variableName: name,
        variableValue: value,
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
