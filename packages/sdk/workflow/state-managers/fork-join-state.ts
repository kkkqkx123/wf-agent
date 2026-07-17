/**
 * ForkJoinState - Manages FORK/JOIN execution state for a workflow execution.
 *
 * Extracted from WorkflowExecutionEntity to reduce its responsibilities.
 * Handles fork path tracking, child execution references, and aggregation state.
 */

import type { WorkflowExecutionStateSnapshot } from "@wf-agent/types";

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
 * Manages fork/join state for a single workflow execution.
 */
export class ForkJoinState {
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
}