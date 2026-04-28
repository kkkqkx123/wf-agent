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
 * - Dependency injection: Receives managers for dependencies through the constructor
 * - Delegation pattern: Uses the VariableState for atomic state operations
 */

import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { VariableScope, WorkflowVariable } from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { RuntimeValidationError, EventSystemError } from "@wf-agent/types";
import { VariableState } from "../../state-managers/variable-state.js";
import { VariableAccessor } from "../utils/variable-accessor.js";
import { safeEmit } from "../../../core/utils/event/event-emitter.js";
import { buildVariableChangedEvent } from "../utils/event/index.js";
import { logError, emitErrorEvent } from "../../../core/utils/error-utils.js";

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
 * - Dependency injection: Receives managers for dependencies through the constructor
 * - Command pattern: Uses the VariableState to perform atomic state operations
 */
export class VariableCoordinator {
  constructor(
    private stateManager: VariableState,
    private eventManager?: EventRegistry,
    private threadId?: string,
    private workflowId?: string,
  ) {}

  /**
   * Initialize variables from WorkflowDefinition
   * @param workflowVariables Workflow variable definitions
   */
  initializeFromWorkflow(workflowVariables: WorkflowVariable[]): void {
    this.stateManager.initializeFromWorkflow(workflowVariables);
  }

  /**
   * Retrieve the value of a variable (searching based on scope priority)
   * Priority: loop > local > thread > global
   * Supports on-demand initialization: Variables in the thread, local, and loop scopes are initialized the first time they are accessed.
   * @param threadEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @returns Variable value
   */
  getVariable(threadEntity: WorkflowExecutionEntity, name: string): unknown {
    const scopes = this.stateManager.getVariableScopes();

    // Loop Scope (Highest Priority)
    if (scopes.loop.length > 0) {
      const currentLoopScope = scopes.loop[scopes.loop.length - 1];
      if (currentLoopScope && name in currentLoopScope) {
        return currentLoopScope[name];
      }
      // If the variable is not initialized, attempt to initialize it as needed.
      if (currentLoopScope && !(name in currentLoopScope)) {
        const initialized = this.initializeVariableOnDemand(name, "loop");
        if (initialized !== undefined) {
          return initialized;
        }
      }
    }

    // 2. Local Scope
    if (scopes.local.length > 0) {
      const currentLocalScope = scopes.local[scopes.local.length - 1];
      if (currentLocalScope && name in currentLocalScope) {
        return currentLocalScope[name];
      }
      // If the variable is not initialized, attempt to initialize it as needed.
      if (currentLocalScope && !(name in currentLocalScope)) {
        const initialized = this.initializeVariableOnDemand(name, "local");
        if (initialized !== undefined) {
          return initialized;
        }
      }
    }

    // 3. Thread Scope
    if (name in scopes.thread) {
      return scopes.thread[name];
    }
    // If the variable is not initialized, attempt to initialize it as needed.
    if (!(name in scopes.thread)) {
      const initialized = this.initializeVariableOnDemand(name, "thread");
      if (initialized !== undefined) {
        return initialized;
      }
    }

    // 4. Global scope (lowest priority)
    if (name in scopes.global) {
      return scopes.global[name];
    }

    return undefined;
  }

  /**
   * Initialize variables as needed
   * @param name: Variable name
   * @param scope: Scope of the variable
   * @returns: The initialized value; returns undefined if the variable does not exist
   */
  private initializeVariableOnDemand(name: string, scope: VariableScope): unknown {
    const variableDef = this.stateManager.getVariableDefinition(name);

    if (!variableDef) {
      return undefined;
    }

    // Initialize with default values
    const initialValue = variableDef.value;
    this.stateManager.setVariableValue(name, initialValue, scope);

    return initialValue;
  }

  /**
   * Update the value of a defined variable
   * @param threadEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @param value New variable value
   * @param explicitScope Explicitly specified scope (optional)
   */
  async updateVariable(
    threadEntity: WorkflowExecutionEntity,
    name: string,
    value: unknown,
    explicitScope?: VariableScope,
  ): Promise<void> {
    const variableDef = this.stateManager.getVariableDefinition(name);

    if (!variableDef) {
      throw new RuntimeValidationError(
        `Variable '${name}' is not defined in workflow. Variables must be defined in WorkflowDefinition.`,
        {
          operation: "setVariable",
          field: "variableName",
          value: name,
          context: { threadId: this.threadId, workflowId: this.workflowId },
        },
      );
    }

    if (variableDef.readonly) {
      throw new RuntimeValidationError(`Variable '${name}' is readonly and cannot be modified`, {
        operation: "setVariable",
        field: "variableName",
        value: name,
        context: { threadId: this.threadId, workflowId: this.workflowId },
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
            threadId: this.threadId,
            workflowId: this.workflowId,
            variableName: name,
            expectedType: variableDef.type,
            actualType: typeof value,
          },
        },
      );
    }

    // If an explicit scope is specified, use that scope.
    const targetScope = explicitScope || variableDef.scope;

    // Delegating atomic operations to the state manager.
    this.stateManager.setVariableValue(name, value, targetScope);

    // Trigger a variable change event
    await this.emitVariableChangedEvent(threadEntity, name, value, targetScope);
  }

  /**
   * Check if the variable exists
   * @param threadEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @returns Whether the variable exists
   */
  hasVariable(threadEntity: WorkflowExecutionEntity, name: string): boolean {
    return this.getVariable(threadEntity, name) !== undefined;
  }

  /**
   * Get all variables (merged by scope priority)
   * @returns A map of all variable key-value pairs
   */
  getAllVariables(): Record<string, unknown> {
    return this.stateManager.getAllVariables();
  }

  /**
   * Get the variable within the specified scope
   * @param scope Variable scope
   * @returns Variable key-value pair within the specified scope
   */
  getVariablesByScope(scope: VariableScope): Record<string, unknown> {
    return this.stateManager.getVariablesByScope(scope);
  }

  /**
   * Enter the local scope
   * Automatically initialize the variables in this scope
   */
  enterLocalScope(): void {
    this.stateManager.enterLocalScope();
  }

  /**
   * Leave the local scope
   */
  exitLocalScope(): void {
    this.stateManager.exitLocalScope();
  }

  /**
   * Enter the scope of the loop
   * Automatically initialize the variables within that scope
   */
  enterLoopScope(): void {
    this.stateManager.enterLoopScope();
  }

  /**
   * Leave the loop scope
   */
  exitLoopScope(): void {
    this.stateManager.exitLoopScope();
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
   * Copy variable (for fork scenarios)
   * @param sourceStateManager Source state manager
   * @param targetStateManager Target state manager
   */
  copyVariables(sourceStateManager: VariableState, targetStateManager: VariableState): void {
    targetStateManager.copyFrom(sourceStateManager);
  }

  /**
   * Clear the variable
   */
  clearVariables(): void {
    this.stateManager.cleanup();
  }

  /**
   * Create a variable accessor
   * Provide a unified interface for accessing variables, supporting nested path resolution
   * @param threadContext: Thread context
   * @returns: VariableAccessor instance
   */
  createAccessor(threadEntity: WorkflowExecutionEntity): VariableAccessor {
    return new VariableAccessor(threadEntity);
  }

  /**
   * 通过路径获取变量值
   * 支持嵌套路径和命名空间
   * @param threadEntity Thread 实体
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
   * getVariableByPath(entity, 'thread.state')
   * getVariableByPath(entity, 'subgraph.temp')
   * getVariableByPath(entity, 'loop.item')
   */
  getVariableByPath(threadEntity: WorkflowExecutionEntity, path: string): unknown {
    const accessor = this.createAccessor(threadEntity);
    return accessor.get(path);
  }

  /**
   * Trigger a variable change event
   * @param threadEntity WorkflowExecutionEntity instance
   * @param name Variable name
   * @param value New value
   * @param scope Scope of the variable
   */
  private async emitVariableChangedEvent(
    threadEntity: WorkflowExecutionEntity,
    name: string,
    value: unknown,
    scope: VariableScope,
  ): Promise<void> {
    if (!this.eventManager) {
      return;
    }

    try {
      const event = buildVariableChangedEvent({
        threadId: workflowExecutionEntity.id,
        workflowId: threadEntity.getWorkflowId(),
        variableName: name,
        variableValue: value,
        variableScope: scope,
      });
      await safeEmit(this.eventManager, event);
    } catch (error) {
      // Error creating the event system.
      const eventSystemError = new EventSystemError(
        "Failed to emit variable changed event",
        "emit",
        "VARIABLE_CHANGED",
        undefined,
        undefined,
        { variableName: name, originalError: getErrorOrNew(error) },
      );

      // Record error logs
      logError(eventSystemError, {
        threadId: this.threadId,
        workflowId: this.workflowId,
        variableName: name,
      });

      // Trigger an error event
      await emitErrorEvent(this.eventManager, {
        threadId: this.threadId || "",
        workflowId: this.workflowId || "",
        error: eventSystemError,
      });

      throw eventSystemError;
    }
  }

  /**
   * Get the state manager (used for scenarios such as checkpoints)
   * @returns VariableState instance
   */
  getStateManager(): VariableState {
    return this.stateManager;
  }
}
