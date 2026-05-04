/**
 * VariableState - Variable State Manager
 * Manages the variable state of the Agent Loop
 *
 * Core responsibilities:
 * 1. manage the runtime state (value) of variables
 * 2. provide instance-isolated state management
 * 3. support state snapshots and restores (for checkpointing)
 * 4. provide atomic state operations
 *
 * Design Principle:
 * - Only state management, no business logic
 * - Instance isolation, each AgentLoopEntity has independent state instances.
 * - Support snapshot and restore
 * - Atomic operations to ensure state consistency
 */

import type { StateManager } from "../../core/types/state-manager.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "VariableState" });

/**
 * Variable State Snapshot
 */
export interface VariableStateSnapshot {
  variables: Record<string, unknown>;
}

/**
 * VariableState - Variable State Manager
 *
 * Responsibilities:
 * - Manage the runtime state of variables
 * - Provides instance-isolated state management
 * - Support state snapshots and restores
 * - Provide atomic state operations
 *
 * Design Principles:
 * - Stateful design: maintain variable state
 * - State Management: Provide add, delete, modify and retrieve operations of state.
 * - Instance isolation: Each AgentLoopEntity has a separate instance of state.
 * - Atomic Operations: Ensure state consistency
 */
export class VariableState implements StateManager<VariableStateSnapshot> {
  private variables: Map<string, unknown> = new Map();

  constructor(private agentLoopId: string) {
    logger.debug("VariableState created", { agentLoopId });
  }

  /**
   * Get the Agent Loop ID
   * @returns Agent Loop ID
   */
  getAgentLoopId(): string {
    return this.agentLoopId;
  }

  /**
   * Getting variables
   * @param name Variable name
   * @returns Variable value
   */
  getVariable(name: string): unknown {
    // Validate input
    if (!name || name.trim() === "") {
      throw new RuntimeValidationError("Variable name cannot be empty", {
        operation: "getVariable",
        field: "name",
      });
    }
    return this.variables.get(name);
  }

  /**
   * Setting variables
   * @param name Variable name
   * @param value Variable value
   */
  setVariable(name: string, value: unknown): void {
    // Validate input
    if (!name || name.trim() === "") {
      throw new RuntimeValidationError("Variable name cannot be empty", {
        operation: "setVariable",
        field: "name",
      });
    }

    logger.debug("Setting variable", {
      agentLoopId: this.agentLoopId,
      variableName: name,
    });
    this.variables.set(name, value);
  }

  /**
   * Get all variables
   * @returns Key-value pairs for all variables
   */
  getAllVariables(): Record<string, unknown> {
    return Object.fromEntries(this.variables);
  }

  /**
   * Deleting variables
   * @param name Variable name
   * @returns Whether the deletion was successful or not
   */
  deleteVariable(name: string): boolean {
    // Validate input
    if (!name || name.trim() === "") {
      throw new RuntimeValidationError("Variable name cannot be empty", {
        operation: "deleteVariable",
        field: "name",
      });
    }

    logger.debug("Deleting variable", {
      agentLoopId: this.agentLoopId,
      variableName: name,
    });
    return this.variables.delete(name);
  }

  /**
   * Checking if a variable exists
   * @param name Variable name
   * @returns if the variable exists
   */
  hasVariable(name: string): boolean {
    // Validate input
    if (!name || name.trim() === "") {
      throw new RuntimeValidationError("Variable name cannot be empty", {
        operation: "hasVariable",
        field: "name",
      });
    }
    return this.variables.has(name);
  }

  /**
   * Getting the number of variables
   * @returns Number of variables
   */
  getVariableCount(): number {
    return this.variables.size;
  }

  /**
   * Creating a Variable Snapshot
   * @returns Variable snapshots
   */
  createSnapshot(): VariableStateSnapshot {
    return {
      variables: Object.fromEntries(this.variables),
    };
  }

  /**
   * Restoring the state of a variable from a snapshot
   * @param snapshot Snapshot of the variable
   */
  restoreFromSnapshot(snapshot: VariableStateSnapshot): void {
    this.variables = new Map(Object.entries(snapshot.variables));
  }

  /**
   * Clear resources
   * Clear all variable states
   */
  cleanup(): void {
    const variableCount = this.variables.size;
    logger.debug("Cleaning up VariableState", {
      agentLoopId: this.agentLoopId,
      variableCount,
    });
    this.variables.clear();
  }

  /**
   * Get the number of variables managed
   * @returns Count of variables
   */
  size(): number {
    return this.variables.size;
  }

  /**
   * Check if the variable state is empty (no variables defined)
   * @returns true if no variables exist
   */
  isEmpty(): boolean {
    return this.variables.size === 0;
  }

  /**
   * Reset to initial state
   * Clears all variables
   */
  reset(): void {
    this.cleanup();
  }
}
