/**
 * Agent Loop Diff Calculator
 *
 * Implements Agent Loop specific diff calculation based on the generic DeltaCalculator.
 */

import {
  DeltaCalculator,
  type DeltaCalculatorContext,
} from "../../core/utils/checkpoint/delta-calculator.js";
import type { AgentLoopStateSnapshot, AgentLoopDelta, Message } from "@wf-agent/types";

/**
 * Agent Loop Diff Calculator
 *
 * Extends the generic DeltaCalculator with Agent Loop specific logic.
 */
export class AgentLoopDiffCalculator extends DeltaCalculator<
  AgentLoopStateSnapshot,
  AgentLoopDelta
> {
  constructor() {
    super({ deepCompare: true });
  }

  /**
   * Calculate delta between two Agent Loop snapshots
   *
   * @param previous Previous snapshot
   * @param current Current snapshot
   * @param context Additional context containing previousMessageCount and currentMessages
   * @returns Delta data
   */
  calculateDelta(
    previous: AgentLoopStateSnapshot,
    current: AgentLoopStateSnapshot,
    context?: DeltaCalculatorContext,
  ): AgentLoopDelta {
    const delta: AgentLoopDelta = {};

    // 1. Calculate message delta (optimized with message count from context)
    const addedMessages = this.calculateMessageDelta(previous, current, context);
    if (addedMessages.length > 0) {
      delta.addedMessages = addedMessages;
    }

    // 2. Calculate status change
    if (previous.status !== current.status) {
      delta.statusChange = {
        from: previous.status,
        to: current.status,
      };
    }

    // 3. Calculate other changes
    const otherChanges = this.calculateOtherChanges(previous, current);
    if (Object.keys(otherChanges).length > 0) {
      delta.otherChanges = otherChanges;
    }

    return delta;
  }

  /**
   * Calculate message delta (optimized version)
   *
   * Uses message count from context for O(1) slicing instead of O(n) comparison.
   *
   * @param previous Previous snapshot
   * @param current Current snapshot
   * @param context Context containing previousMessageCount and currentMessages
   * @returns Added messages
   */
  private calculateMessageDelta(
    previous: AgentLoopStateSnapshot,
    current: AgentLoopStateSnapshot,
    context?: DeltaCalculatorContext,
  ): Message[] {
    // Optimization: If context provides message count, use it for direct slicing
    if (context?.previousMessageCount !== undefined && context?.currentMessages) {
      if (context.currentMessages.length > context.previousMessageCount) {
        return context.currentMessages.slice(context.previousMessageCount) as Message[];
      }
      return [];
    }

    // Fallback: Use generic array delta calculation
    return this.calculateArrayDelta(previous.messages, current.messages) as Message[];
  }

  /**
   * Calculate other state changes specific to Agent Loop
   */
  private calculateOtherChanges(
    previous: AgentLoopStateSnapshot,
    current: AgentLoopStateSnapshot,
  ): Record<string, { from: unknown; to: unknown }> {
    const otherChanges: Record<string, { from: unknown; to: unknown }> = {};

    // Tool call count change
    if (previous.toolCallCount !== current.toolCallCount) {
      otherChanges["toolCallCount"] = {
        from: previous.toolCallCount,
        to: current.toolCallCount,
      };
    }

    // Error change
    if (!this.isEqual(previous.error, current.error)) {
      otherChanges["error"] = {
        from: previous.error,
        to: current.error,
      };
    }

    // Time changes
    if (previous.startTime !== current.startTime) {
      otherChanges["startTime"] = {
        from: previous.startTime,
        to: current.startTime,
      };
    }

    if (previous.endTime !== current.endTime) {
      otherChanges["endTime"] = {
        from: previous.endTime,
        to: current.endTime,
      };
    }

    return otherChanges;
  }
}
