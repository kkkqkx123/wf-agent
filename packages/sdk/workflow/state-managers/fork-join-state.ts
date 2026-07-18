/**
 * ForkJoinState - Manages FORK/JOIN execution state for a workflow execution.
 *
 * Extracted from WorkflowExecutionEntity to reduce its responsibilities.
 * Handles fork path tracking, child execution references, and aggregation state.
 * Implements StateManager for unified checkpoint serialization.
 */

import type { WorkflowExecutionStateSnapshot } from "@wf-agent/types";
import type { StateManager } from "../../shared/types/state-manager.js";

/**
 * Fork/join context for tracking branch execution
 */
export interface ForkJoinContext {
  /** The FORK node ID that spawned the branches */
  forkId: string;
  /** The path ID for this branch */
  forkPathId: string;
}

/**
 * Snapshot type for ForkJoinState
 */
export interface ForkJoinStateSnapshot {
  /** Fork/join branch context */
  forkJoinContext?: ForkJoinContext;
  /** FORK/JOIN aggregation state for JOIN node result merging */
  forkJoinAggregationState?: WorkflowExecutionStateSnapshot["forkJoinAggregationState"];
}

/**
 * Manages fork/join state for a single workflow execution.
 * Implements StateManager for unified checkpoint serialization.
 */
export class ForkJoinState implements StateManager<ForkJoinStateSnapshot> {
  /** Fork/join context for tracking branch execution */
  private forkJoinContext?: ForkJoinContext;

  /** FORK/JOIN aggregation state for JOIN node result merging */
  private forkJoinAggregationState?: WorkflowExecutionStateSnapshot["forkJoinAggregationState"];

  /**
   * Set the fork ID for this execution.
   */
  setForkId(forkId: string): void {
    if (!this.forkJoinContext) {
      this.forkJoinContext = { forkId, forkPathId: "" };
    }
    this.forkJoinContext.forkId = forkId;
  }

  /**
   * Get the fork ID for this execution.
   */
  getForkId(): string | undefined {
    return this.forkJoinContext?.forkId;
  }

  /**
   * Set the fork path ID for this execution.
   */
  setForkPathId(forkPathId: string): void {
    if (!this.forkJoinContext) {
      this.forkJoinContext = { forkId: "", forkPathId };
    }
    this.forkJoinContext.forkPathId = forkPathId;
  }

  /**
   * Get the fork path ID for this execution.
   */
  getForkPathId(): string | undefined {
    return this.forkJoinContext?.forkPathId;
  }

  /**
   * Get the full fork/join context.
   */
  getForkJoinContext(): ForkJoinContext | undefined {
    return this.forkJoinContext;
  }

  /**
   * Get the FORK/JOIN aggregation state.
   */
  getAggregationState(): WorkflowExecutionStateSnapshot["forkJoinAggregationState"] {
    return this.forkJoinAggregationState;
  }

  /**
   * Set the FORK/JOIN aggregation state.
   */
  setAggregationState(
    state: NonNullable<WorkflowExecutionStateSnapshot["forkJoinAggregationState"]>,
  ): void {
    this.forkJoinAggregationState = state;
  }

  /**
   * Restore the FORK/JOIN aggregation state from a snapshot.
   */
  restoreAggregationState(
    state: WorkflowExecutionStateSnapshot["forkJoinAggregationState"],
  ): void {
    this.forkJoinAggregationState = state;
  }

  /**
   * Check if this execution is part of a fork branch.
   */
  isForkBranch(): boolean {
    return this.forkJoinContext !== undefined && this.forkJoinContext.forkId !== "";
  }

  // ============================================================
  // StateManager Implementation
  // ============================================================

  /**
   * Clean up resources
   */
  cleanup(): void {
    this.forkJoinContext = undefined;
    this.forkJoinAggregationState = undefined;
  }

  /**
   * Create a snapshot of the current fork/join state
   * @returns ForkJoinState snapshot
   */
  createSnapshot(): ForkJoinStateSnapshot {
    return {
      forkJoinContext: this.forkJoinContext ? { ...this.forkJoinContext } : undefined,
      forkJoinAggregationState: this.forkJoinAggregationState
        ? { ...this.forkJoinAggregationState }
        : undefined,
    };
  }

  /**
   * Restore fork/join state from a snapshot
   * @param snapshot The state snapshot
   */
  restoreFromSnapshot(snapshot: ForkJoinStateSnapshot): void {
    this.forkJoinContext = snapshot.forkJoinContext
      ? { ...snapshot.forkJoinContext }
      : undefined;
    this.forkJoinAggregationState = snapshot.forkJoinAggregationState
      ? { ...snapshot.forkJoinAggregationState }
      : undefined;
  }

  /**
   * Get the number of state items managed
   * @returns 1 if any state exists, 0 otherwise
   */
  size(): number {
    return this.forkJoinContext || this.forkJoinAggregationState ? 1 : 0;
  }

  /**
   * Check if the fork/join state is empty
   * @returns true if no fork/join state exists
   */
  isEmpty(): boolean {
    return !this.forkJoinContext && !this.forkJoinAggregationState;
  }

  /**
   * Reset to initial state
   */
  reset(): void {
    this.cleanup();
  }
}