/**
 * VariableState - Variable State Manager
 * Specializes in managing the runtime state of variables, separating it from the logic that coordinates with the variables.
 *
 * Core Responsibilities:
 * 1. Manage the runtime state of variables (values, scopes)
 * 2. Provide workflow-execution-isolated state management
 * 3. Support state snapshots and restoration (for use in checkpoints)
 * 4. Offer atomic state operations
 *
 * Design Principles:
 * - Only manage state; do not include business logic
 * - WorkflowExecution isolation, with each workflow execution having its own independent state instance
 * - Support for snapshots and state restoration
 * - Atomic operations to ensure state consistency
 */

import type { WorkflowExecutionVariable } from "@wf-agent/types";
import type { VariableScope, VariableScopes } from "@wf-agent/types";
import type { WorkflowVariable } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { LifecycleCapable } from "../../core/types/lifecycle-capable.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "variable-state-manager" });

/**
 * VariableState - Variable State Manager
 *
 * Responsibilities:
 * - Manage the runtime state of variables
 * - Provide workflow-execution-isolated state management
 * - Support state snapshots and restoration
 * - Offer atomic state operations
 *
 * Design Principles:
 * - Stateful design: Maintain the state of variables
 * - State management: Provide operations for creating, reading, updating, and deleting states
 * - WorkflowExecution isolation: Each workflow execution has its own independent state instance
 * - Atomic operations: Ensure state consistency
 */
export class VariableState implements LifecycleCapable<{
  variables: WorkflowExecutionVariable[];
  variableScopes: VariableScopes;
}> {
  private variables: WorkflowExecutionVariable[] = [];
  private variableScopes: VariableScopes = {
    global: {},
    workflowExecution: {},
    local: [],
    loop: [],
  };

  /**
   * Constructor
   * @param executionId Execution ID (optional, for logging/debugging purposes)
   */
  constructor(executionId?: string) {
    // executionId is stored for potential future use (logging, debugging)
    // Currently not used but kept for API compatibility
  }

  /**
   * Initialize variable states from WorkflowTemplate
   * @param workflowVariables: Workflow variable definitions
   */
  initializeFromWorkflow(workflowVariables: WorkflowVariable[]): void {
    if (!workflowVariables || workflowVariables.length === 0) {
      this.variables = [];
      this.variableScopes = {
        global: {},
        workflowExecution: {},
        local: [],
        loop: [],
      };
      return;
    }

    // Create a WorkflowExecutionVariable from WorkflowVariable
    this.variables = workflowVariables.map(
      (v: WorkflowVariable): WorkflowExecutionVariable => ({
        name: v.name,
        value: v.defaultValue,
        type: v.type,
        scope: v.scope || "workflowExecution",
        readonly: v.readonly || false,
        metadata: {
          description: v.description,
          required: v.required,
        },
      }),
    );

    // Initialize a level 4 scope
    this.variableScopes = {
      global: {},
      workflowExecution: {},
      local: [],
      loop: [],
    };

    // Assign variable values based on their scope.
    // Only variables with global scope are assigned values directly during initialization.
    // Variables in the workflowExecution, local, and loop scopes are initialized as needed.
    for (const variable of this.variables) {
      switch (variable.scope) {
        case "global":
          // Global scope variables are initialized immediately.
          this.variableScopes.global[variable.name] = variable.value;
          break;
        case "workflowExecution":
        case "local":
        case "loop":
          // Variables in the workflowExecution, local, and loop scopes are initialized as needed.
          // This section only contains declarations; no values are initialized.
          break;
      }
    }
  }

  /**
   * Initialize variable state from WorkflowExecutionVariable
   * @param workflowExecutionVariables Definition of WorkflowExecution variables
   */
  initializeFromWorkflowExecutionVariables(workflowExecutionVariables: WorkflowExecutionVariable[]): void {
    if (!workflowExecutionVariables || workflowExecutionVariables.length === 0) {
      this.variables = [];
      this.variableScopes = {
        global: {},
        workflowExecution: {},
        local: [],
        loop: [],
      };
      return;
    }

    // Use WorkflowExecutionVariable directly
    this.variables = workflowExecutionVariables.map((v: WorkflowExecutionVariable): WorkflowExecutionVariable => ({ ...v }));

    // Initialize a level-4 scope
    this.variableScopes = {
      global: {},
      workflowExecution: {},
      local: [],
      loop: [],
    };

    // Assign variable values based on their scope.
    // Only variables with global scope are assigned values directly upon initialization.
    // Variables in the workflowExecution, local, and loop scopes are initialized as needed.
    for (const variable of this.variables) {
      switch (variable.scope) {
        case "global":
          // Global scope variables are initialized immediately.
          this.variableScopes.global[variable.name] = variable.value;
          break;
        case "workflowExecution":
        case "local":
        case "loop":
          // Variables in the workflowExecution, local, and loop scopes are initialized as needed.
          // This section only makes declarations; no values are initialized.
          break;
      }
    }
  }

  /**
   * Get variable definition
   * @param name Variable name
   * @returns Variable definition; returns undefined if the variable does not exist
   */
  getVariableDefinition(name: string): WorkflowExecutionVariable | undefined {
    return this.variables.find(v => v.name === name);
  }

  /**
   * Get all variable definitions
   * @returns All variable definitions
   */
  getAllVariableDefinitions(): WorkflowExecutionVariable[] {
    return [...this.variables];
  }

  /**
   * Set variable value (atomic operation)
   * @param name Variable name
   * @param value Variable value
   * @param scope Scope
   */
  setVariableValue(name: string, value: unknown, scope: VariableScope): void {
    logger.debug("Setting variable value", { name, scope });

    switch (scope) {
      case "global":
        this.variableScopes.global[name] = value;
        break;
      case "workflowExecution":
        this.variableScopes.workflowExecution[name] = value;
        break;
      case "local": {
        if (this.variableScopes.local.length === 0) {
          throw new RuntimeValidationError(
            "Cannot set local variable outside of local scope context",
            { operation: "setVariable", field: "scope" },
          );
        }
        const localScope = this.variableScopes.local[this.variableScopes.local.length - 1];
        if (localScope) {
          localScope[name] = value;
        }
        break;
      }
      case "loop": {
        if (this.variableScopes.loop.length === 0) {
          throw new RuntimeValidationError("Cannot set loop variable outside of loop context", {
            operation: "setVariable",
            field: "scope",
          });
        }
        const loopScope = this.variableScopes.loop[this.variableScopes.loop.length - 1];
        if (loopScope) {
          loopScope[name] = value;
        }
        break;
      }
    }

    // Update the value in the variable definition.
    const variableDef = this.variables.find(v => v.name === name);
    if (variableDef) {
      variableDef.value = value;
    }
  }

  /**
   * Get the variable value (atomic operation)
   * @param name: Variable name
   * @param scope: Scope of the variable
   * @returns: Variable value
   */
  getVariableValue(name: string, scope: VariableScope): unknown {
    switch (scope) {
      case "global":
        return this.variableScopes.global[name];
      case "workflowExecution":
        return this.variableScopes.workflowExecution[name];
      case "local": {
        if (this.variableScopes.local.length === 0) {
          return undefined;
        }
        const localScope = this.variableScopes.local[this.variableScopes.local.length - 1];
        return localScope ? localScope[name] : undefined;
      }
      case "loop": {
        if (this.variableScopes.loop.length === 0) {
          return undefined;
        }
        const loopScope = this.variableScopes.loop[this.variableScopes.loop.length - 1];
        return loopScope ? loopScope[name] : undefined;
      }
    }
    return undefined;
  }

  /**
   * Enter local scope (atomic operation)
   */
  enterLocalScope(): void {
    logger.debug("Entering local scope", { currentDepth: this.variableScopes.local.length });

    const newScope: Record<string, unknown> = {};

    // Initialize all local variables in this scope.
    for (const variable of this.variables) {
      if (variable.scope === "local") {
        newScope[variable.name] = variable.value;
      }
    }

    this.variableScopes.local.push(newScope);
  }

  /**
   * Exit local scope (atomic operation)
   */
  exitLocalScope(): void {
    if (this.variableScopes.local.length === 0) {
      throw new RuntimeValidationError("No local scope to exit", {
        operation: "exitLocalScope",
        field: "scope",
      });
    }
    this.variableScopes.local.pop();
    logger.debug("Exited local scope", { newDepth: this.variableScopes.local.length });
  }

  /**
   * Enter the loop scope (atomic operation)
   */
  enterLoopScope(): void {
    logger.debug("Entering loop scope", { currentDepth: this.variableScopes.loop.length });

    const newScope: Record<string, unknown> = {};

    // Initialize all loop variables in this scope.
    for (const variable of this.variables) {
      if (variable.scope === "loop") {
        newScope[variable.name] = variable.value;
      }
    }

    this.variableScopes.loop.push(newScope);
  }

  /**
   * Exiting the scope of a loop (atomic operation)
   */
  exitLoopScope(): void {
    if (this.variableScopes.loop.length === 0) {
      throw new RuntimeValidationError("No loop scope to exit", {
        operation: "exitLoopScope",
        field: "scope",
      });
    }
    this.variableScopes.loop.pop();
    logger.debug("Exited loop scope", { newDepth: this.variableScopes.loop.length });
  }

  /**
   * Get all variables (merged by scope priority)
   * @returns A dictionary of all variable key-value pairs
   */
  getAllVariables(): Record<string, unknown> {
    const allVariables: Record<string, unknown> = {};

    // Merge from lowest to highest scope priority (global -> workflowExecution -> subgraph -> loop).
    // Variables with higher priority scopes will override variables with the same name but in lower priority scopes.

    // 1. Global scope (lowest priority)
    Object.assign(allVariables, this.variableScopes.global);

    // 2. WorkflowExecution Scope
    Object.assign(allVariables, this.variableScopes.workflowExecution);

    // 3. Local scope (from outer to inner, with inner scopes overriding outer scopes)
    for (const localScope of this.variableScopes.local) {
      Object.assign(allVariables, localScope);
    }

    // 4. Loop Scope (From outer to inner, inner loops override outer loops, with the highest priority)
    for (const loopScope of this.variableScopes.loop) {
      Object.assign(allVariables, loopScope);
    }

    return allVariables;
  }

  /**
   * Get the variable within the specified scope
   * @param scope: The scope of the variable
   * @returns: A key-value pair of the variable within the specified scope
   */
  getVariablesByScope(scope: VariableScope): Record<string, unknown> {
    switch (scope) {
      case "global":
        return { ...this.variableScopes.global };
      case "workflowExecution":
        return { ...this.variableScopes.workflowExecution };
      case "local":
        if (this.variableScopes.local.length === 0) {
          return {};
        }
        return { ...this.variableScopes.local[this.variableScopes.local.length - 1] };
      case "loop":
        if (this.variableScopes.loop.length === 0) {
          return {};
        }
        return { ...this.variableScopes.loop[this.variableScopes.loop.length - 1] };
    }
    return {};
  }

  /**
   * Create a variable snapshot
   * Used to save the complete state of a variable, including its definition and scope structure
   * @returns Variable snapshot
   */
  createSnapshot(): {
    variables: WorkflowExecutionVariable[];
    variableScopes: VariableScopes;
  } {
    return {
      variables: this.variables.map(v => ({ ...v })),
      variableScopes: {
        global: { ...this.variableScopes.global },
        workflowExecution: { ...this.variableScopes.workflowExecution },
        local: this.variableScopes.local.map(scope => ({ ...scope })),
        loop: this.variableScopes.loop.map(scope => ({ ...scope })),
      },
    };
  }

  /**
   * Restore variable snapshot
   * Recover the complete state of a variable from the snapshot
   * @param snapshot Variable snapshot
   */
  restoreFromSnapshot(snapshot: {
    variables: WorkflowExecutionVariable[];
    variableScopes: VariableScopes;
  }): void {
    this.variables = snapshot.variables.map(v => ({ ...v }));
    this.variableScopes = {
      global: { ...snapshot.variableScopes.global },
      workflowExecution: { ...snapshot.variableScopes.workflowExecution },
      local: snapshot.variableScopes.local.map(scope => ({ ...scope })),
      loop: snapshot.variableScopes.loop.map(scope => ({ ...scope })),
    };
  }

  /**
   * Copy variable state (for fork scenarios)
   * @param sourceStateManager Source state manager
   */
  copyFrom(sourceStateManager: VariableState): void {
    this.variables = sourceStateManager.variables.map((v: WorkflowExecutionVariable) => ({ ...v }));

    // The global scope is shared through references.
    this.variableScopes = {
      global: sourceStateManager.variableScopes.global,
      workflowExecution: { ...sourceStateManager.variableScopes.workflowExecution },
      local: [],
      loop: [],
    };
  }

  /**
   * Get the variable scope structure
   * @returns Variable scope structure
   */
  getVariableScopes(): VariableScopes {
    return {
      global: { ...this.variableScopes.global },
      workflowExecution: { ...this.variableScopes.workflowExecution },
      local: this.variableScopes.local.map(scope => ({ ...scope })),
      loop: this.variableScopes.loop.map(scope => ({ ...scope })),
    };
  }

  /**
   * Get variable value by name (searches all scopes)
   * @param name Variable name
   * @returns Variable value or undefined if not found
   */
  getVariable(name: string): unknown {
    // Check workflowExecution scope first
    if (name in this.variableScopes.workflowExecution) {
      return this.variableScopes.workflowExecution[name];
    }

    // Check local scope (current level)
    if (this.variableScopes.local.length > 0) {
      const localScope = this.variableScopes.local[this.variableScopes.local.length - 1];
      if (localScope && name in localScope) {
        return localScope[name];
      }
    }

    // Check loop scope (current level)
    if (this.variableScopes.loop.length > 0) {
      const loopScope = this.variableScopes.loop[this.variableScopes.loop.length - 1];
      if (loopScope && name in loopScope) {
        return loopScope[name];
      }
    }

    // Check global scope
    if (name in this.variableScopes.global) {
      return this.variableScopes.global[name];
    }

    return undefined;
  }

  /**
   * Set variable value by name (uses variable's defined scope)
   * @param name Variable name
   * @param value Variable value
   */
  setVariable(name: string, value: unknown): void {
    const variableDef = this.variables.find(v => v.name === name);
    if (variableDef) {
      this.setVariableValue(name, value, variableDef.scope);
    } else {
      // If variable doesn't exist, default to workflowExecution scope
      this.setVariableValue(name, value, "workflowExecution");
    }
  }

  /**
   * Delete variable
   * @param name: Variable name
   * @returns Whether the deletion was successful
   */
  deleteVariable(name: string): boolean {
    logger.debug("Deleting variable", { variableName: name });

    // Find and remove from variables array
    const index = this.variables.findIndex(v => v.name === name);
    if (index !== -1) {
      this.variables.splice(index, 1);
    }

    // Remove from all scopes
    let deleted = false;
    if (name in this.variableScopes.global) {
      delete this.variableScopes.global[name];
      deleted = true;
    }
    if (name in this.variableScopes.workflowExecution) {
      delete this.variableScopes.workflowExecution[name];
      deleted = true;
    }
    for (const localScope of this.variableScopes.local) {
      if (name in localScope) {
        delete localScope[name];
        deleted = true;
      }
    }
    for (const loopScope of this.variableScopes.loop) {
      if (name in loopScope) {
        delete loopScope[name];
        deleted = true;
      }
    }

    return deleted || index !== -1;
  }

  /**
   * Clean up resources
   * Clear all variable states and scopes
   */
  cleanup(): void {
    this.variables = [];
    this.variableScopes = {
      global: {},
      workflowExecution: {},
      local: [],
      loop: [],
    };
  }
}
