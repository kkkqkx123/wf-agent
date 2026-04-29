/**
 * TriggerState - A trigger state manager that specifically manages the runtime status of triggers, decoupled from the trigger definitions.
 *
 * Core responsibilities:
 * 1. Manage the runtime status of triggers (enabled/disabled, number of triggers fired, etc.)
 * 2. Provide thread-isolated state management
 * 3. Support state snapshots and restoration (for use as checkpoints)
 * 4. Ensure concurrent safety
 *
 * Design principles:
 * - Only manage the state, not the trigger definitions
 * - Thread isolation, with each thread having its own independent state
 * - Support for snapshots and restoration
 * - Concurrency safety
 *
 */

import type { TriggerStatus, TriggerRuntimeState } from "@wf-agent/types";
import type { ID } from "@wf-agent/types";
import { ExecutionError, NotFoundError, RuntimeValidationError } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import type { LifecycleCapable } from "../../core/types/lifecycle-capable.js";

export type { TriggerRuntimeState };

/**
 * TriggerState - Trigger State Manager
 *
 * Responsibilities:
 * - Manage the runtime state of triggers
 * - Provide thread-isolated state management
 * - Support state snapshots and restoration
 * - Ensure concurrent safety
 */
export class TriggerState implements LifecycleCapable<Map<ID, TriggerRuntimeState>> {
  private states: Map<ID, TriggerRuntimeState> = new Map();
  private executionId: ID;
  private workflowId: ID | null = null;

  constructor(executionId: ID) {
    this.executionId = executionId;
  }

  /**
   * Set the workflow ID
   * @param workflowId: The workflow ID
   */
  setWorkflowId(workflowId: ID): void {
    this.workflowId = workflowId;
  }

  /**
   * Get the workflow ID
   * @returns Workflow ID
   */
  getWorkflowId(): ID | null {
    return this.workflowId;
  }

  /**
   * Get execution ID
   * @returns Execution ID
   */
  getExecutionId(): ID {
    return this.executionId;
  }

  /**
   * Register Trigger Status
   * @param state: The trigger's runtime status
   */
  register(state: TriggerRuntimeState): void {
    if (!state.triggerId) {
      throw new RuntimeValidationError("Trigger ID cannot be null", {
        operation: "register",
        field: "triggerId",
      });
    }
    if (!state.executionId) {
      throw new RuntimeValidationError("Execution ID cannot be null", {
        operation: "register",
        field: "executionId",
      });
    }
    if (!state.workflowId) {
      throw new RuntimeValidationError("Workflow ID cannot be null", {
        operation: "register",
        field: "workflowId",
      });
    }
    if (state.executionId !== this.executionId) {
      throw new RuntimeValidationError(
        `Execution ID mismatch: expected ${this.executionId}, actual ${state.executionId}`,
        { operation: "register", field: "executionId", value: state.executionId },
      );
    }
    if (this.workflowId && state.workflowId !== this.workflowId) {
      throw new RuntimeValidationError(
        `Workflow ID mismatch: expected ${this.workflowId}, actual ${state.workflowId}`,
        { operation: "register", field: "workflowId", value: state.workflowId },
      );
    }

    if (this.states.has(state.triggerId)) {
      throw new ExecutionError(`Trigger state ${state.triggerId} Existing`);
    }

    this.states.set(state.triggerId, {
      triggerId: state.triggerId,
      executionId: state.executionId,
      workflowId: state.workflowId,
      status: state.status,
      triggerCount: state.triggerCount,
      updatedAt: state.updatedAt,
    });
  }

  /**
   * Get trigger status
   * @param triggerId: Trigger ID
   * @returns: Trigger's runtime status; returns undefined if the trigger does not exist
   */
  getState(triggerId: ID): TriggerRuntimeState | undefined {
    return this.states.get(triggerId);
  }

  /**
   * Update trigger status
   * @param triggerId Trigger ID
   * @param status New status
   */
  updateStatus(triggerId: ID, status: TriggerStatus): void {
    const state = this.states.get(triggerId);
    if (!state) {
      throw new NotFoundError(`Trigger status ${triggerId} not present`, "TriggerState", triggerId);
    }

    state.status = status;
    state.updatedAt = now();
  }

  /**
   * Increase the number of triggers
   * @param triggerId Trigger ID
   */
  incrementTriggerCount(triggerId: ID): void {
    const state = this.states.get(triggerId);
    if (!state) {
      throw new NotFoundError(`Trigger status ${triggerId} not present`, "TriggerState", triggerId);
    }

    state.triggerCount++;
    state.updatedAt = now();
  }

  /**
   * Create a status snapshot
   * @returns Status snapshot
   */
  createSnapshot(): Map<ID, TriggerRuntimeState> {
    const snapshot = new Map<ID, TriggerRuntimeState>();
    for (const [triggerId, state] of this.states.entries()) {
      snapshot.set(triggerId, {
        triggerId: state.triggerId,
        executionId: state.executionId,
        workflowId: state.workflowId,
        status: state.status,
        triggerCount: state.triggerCount,
        updatedAt: state.updatedAt,
      });
    }
    return snapshot;
  }

  /**
   * Restore from a snapshot state
   * @param snapshot The state snapshot
   */
  restoreFromSnapshot(snapshot: Map<ID, TriggerRuntimeState>): void {
    this.states.clear();

    for (const [triggerId, state] of snapshot.entries()) {
      if (state.executionId !== this.executionId) {
        throw new RuntimeValidationError(
          `Execution ID mismatch: expected ${this.executionId}, actual ${state.executionId}`,
          { operation: "update", field: "executionId", value: state.executionId },
        );
      }

      this.states.set(triggerId, {
        triggerId: state.triggerId,
        executionId: state.executionId,
        workflowId: state.workflowId,
        status: state.status,
        triggerCount: state.triggerCount,
        updatedAt: state.updatedAt,
      });
    }
  }

  /**
   * Get all statuses
   * @returns All trigger runtime statuses
   */
  getAllStates(): Map<ID, TriggerRuntimeState> {
    const readonlyStates = new Map<ID, TriggerRuntimeState>();
    for (const [triggerId, state] of this.states.entries()) {
      readonlyStates.set(triggerId, { ...state });
    }
    return readonlyStates;
  }

  /**
   * Check if the trigger status exists
   * @param triggerId Trigger ID
   * @returns Whether it exists
   */
  hasState(triggerId: ID): boolean {
    return this.states.has(triggerId);
  }

  /**
   * Delete trigger status
   * @param triggerId Trigger ID
   */
  deleteState(triggerId: ID): void {
    if (!this.states.has(triggerId)) {
      throw new NotFoundError(`Trigger status ${triggerId} not present`, "TriggerState", triggerId);
    }
    this.states.delete(triggerId);
  }

  /**
   * Get the number of statuses
   * @returns Number of statuses
   */
  size(): number {
    return this.states.size;
  }

  /**
   * Clean up resources
   * Clear all trigger states
   */
  cleanup(): void {
    this.states.clear();
  }
}
