/**
 * Checkpoint Diff Calculator
 *
 * Implements Workflow/WorkflowExecution specific diff calculation based on the generic DeltaCalculator.
 */

import {
  DeltaCalculator,
  type DeltaCalculatorContext,
} from "../../../core/utils/checkpoint/delta-calculator.js";
import type { ThreadStateSnapshot, CheckpointDelta, NodeExecutionResult } from "@wf-agent/types";

/**
 * Checkpoint Diff Calculator
 *
 * Extends the generic DeltaCalculator with Workflow/WorkflowExecution specific logic.
 */
export class CheckpointDiffCalculator extends DeltaCalculator<
  ThreadStateSnapshot,
  CheckpointDelta
> {
  constructor() {
    super({ deepCompare: true });
  }

  /**
   * Calculate delta between two WorkflowExecution snapshots
   *
   * @param previous Previous snapshot
   * @param current Current snapshot
   * @returns Delta data
   */
  calculateDelta(
    previous: ThreadStateSnapshot,
    current: ThreadStateSnapshot,
    _context?: DeltaCalculatorContext,
  ): CheckpointDelta {
    const delta: CheckpointDelta = {};

    // 1. Calculate message delta
    const addedMessages = this.calculateArrayDelta(
      previous.conversationState.messages,
      current.conversationState.messages,
    );
    if (addedMessages.length > 0) {
      delta.addedMessages = addedMessages;
    }

    // 2. Calculate variable delta
    const varDiff = this.calculateObjectDelta(
      previous.variables as unknown as Record<string, unknown>,
      current.variables as unknown as Record<string, unknown>,
    );
    if (varDiff.added.length > 0) {
      delta.addedVariables = varDiff.added;
    }
    if (varDiff.modified.size > 0) {
      delta.modifiedVariables = varDiff.modified;
    }

    // 3. Calculate node results delta
    const addedNodeResults = this.calculateNodeResultsDelta(
      previous.nodeResults,
      current.nodeResults,
    );
    if (Object.keys(addedNodeResults).length > 0) {
      delta.addedNodeResults = addedNodeResults;
    }

    // 4. Calculate status change
    if (previous.status !== current.status) {
      delta.statusChange = { from: previous.status, to: current.status };
    }

    // 5. Calculate current node change
    if (previous.currentNodeId !== current.currentNodeId) {
      delta.currentNodeChange = {
        from: previous.currentNodeId,
        to: current.currentNodeId,
      };
    }

    // 6. Calculate other changes
    const otherChanges = this.calculateOtherChanges(previous, current);
    if (Object.keys(otherChanges).length > 0) {
      delta.otherChanges = otherChanges;
    }

    return delta;
  }

  /**
   * Calculate node results delta
   */
  private calculateNodeResultsDelta(
    previous: Record<string, NodeExecutionResult>,
    current: Record<string, NodeExecutionResult>,
  ): Record<string, NodeExecutionResult> {
    const added: Record<string, NodeExecutionResult> = {};

    for (const [nodeId, result] of Object.entries(current)) {
      if (!(nodeId in previous)) {
        added[nodeId] = result;
      }
    }

    return added;
  }

  /**
   * Calculate other state changes specific to WorkflowExecution
   */
  private calculateOtherChanges(
    previous: ThreadStateSnapshot,
    current: ThreadStateSnapshot,
  ): Record<string, { from: unknown; to: unknown }> {
    const changes: Record<string, { from: unknown; to: unknown }> = {};

    // Compare output field
    if (!this.isEqual(previous.output, current.output)) {
      changes["output"] = { from: previous.output, to: current.output };
    }

    // Compare errors field
    if (!this.isEqual(previous.errors, current.errors)) {
      changes["errors"] = { from: previous.errors, to: current.errors };
    }

    return changes;
  }
}
