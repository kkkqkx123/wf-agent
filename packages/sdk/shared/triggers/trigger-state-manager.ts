/**
 * Trigger State Manager
 *
 * Manages persistent state for triggers across executions.
 * Tracks trigger fire count, last fired time, and other metadata.
 * State can be serialized to checkpoints and restored.
 */

import type { BaseTriggerDefinition } from "./types.js";

/**
 * State for a single trigger
 */
export interface TriggerState {
  /** Trigger ID */
  triggerId: string;
  /** Number of times this trigger has fired */
  fireCount: number;
  /** Timestamp of last fire */
  lastFiredAt?: number;
  /** Additional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Trigger State Manager
 *
 * Manages and persists trigger state across checkpoint cycles.
 */
export class TriggerStateManager {
  /** Map of trigger ID to trigger state */
  private state: Map<string, TriggerState> = new Map();

  /**
   * Initialize state from persisted data
   */
  constructor(initialState?: Record<string, TriggerState>) {
    if (initialState) {
      for (const [triggerId, state] of Object.entries(initialState)) {
        this.state.set(triggerId, state);
      }
    }
  }

  /**
   * Get state for a trigger
   */
  getState(triggerId: string): TriggerState | undefined {
    return this.state.get(triggerId);
  }

  /**
   * Set state for a trigger
   */
  setState(triggerId: string, state: TriggerState): void {
    this.state.set(triggerId, state);
  }

  /**
   * Increment fire count for a trigger
   */
  incrementFireCount(triggerId: string): number {
    const state = this.state.get(triggerId) || {
      triggerId,
      fireCount: 0,
    };
    state.fireCount += 1;
    state.lastFiredAt = Date.now();
    this.state.set(triggerId, state);
    return state.fireCount;
  }

  /**
   * Check if a trigger has reached its limit
   */
  hasReachedLimit(trigger: BaseTriggerDefinition): boolean {
    if (!trigger.maxTriggers || trigger.maxTriggers <= 0) {
      return false;
    }

    const state = this.state.get(trigger.id);
    const fireCount = state?.fireCount || trigger.triggerCount || 0;
    return fireCount >= trigger.maxTriggers;
  }

  /**
   * Get remaining triggers for a trigger
   */
  getRemainingTriggers(trigger: BaseTriggerDefinition): number {
    if (!trigger.maxTriggers || trigger.maxTriggers <= 0) {
      return -1; // Unlimited
    }

    const state = this.state.get(trigger.id);
    const fireCount = state?.fireCount || trigger.triggerCount || 0;
    return Math.max(0, trigger.maxTriggers - fireCount);
  }

  /**
   * Reset state for a trigger
   */
  reset(triggerId: string): void {
    this.state.delete(triggerId);
  }

  /**
   * Reset all state
   */
  resetAll(): void {
    this.state.clear();
  }

  /**
   * Export state to persistent format
   * Returns deep copies to prevent reference sharing issues
   */
  toJSON(): Record<string, TriggerState> {
    const result: Record<string, TriggerState> = {};
    for (const [triggerId, state] of this.state.entries()) {
      result[triggerId] = { ...state };
    }
    return result;
  }

  /**
   * Import state from persistent format
   * Creates deep copies to prevent reference sharing issues
   */
  static fromJSON(data: Record<string, TriggerState>): TriggerStateManager {
    const copiedData: Record<string, TriggerState> = {};
    for (const [triggerId, state] of Object.entries(data)) {
      copiedData[triggerId] = { ...state };
    }
    return new TriggerStateManager(copiedData);
  }
}
