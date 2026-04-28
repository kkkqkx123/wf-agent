/**
 * Node Hook Related Type Definitions
 */

import type { Condition } from "../graph/condition.js";

/**
 * Hook Type
 */
export type HookType =
  /** Triggered before node execution */
  | "BEFORE_EXECUTE"
  /** Triggered after node execution */
  | "AFTER_EXECUTE";

/**
 * Node Hook Configuration
 */
export interface NodeHook {
  /** Hook Type */
  hookType: HookType;
  /** Trigger condition expression (optional) */
  condition?: Condition;
  /** Name of the custom event to trigger */
  eventName: string;
  /** Event load generation logic (optional) */
  eventPayload?: Record<string, unknown>;
  /** Enable or not (default true) */
  enabled?: boolean;
  /** Weighting (the higher the number the higher the priority) */
  weight?: number;
  /** Whether to create checkpoints when Hook is triggered (new) */
  createCheckpoint?: boolean;
  /** Description of checkpoints (new) */
  checkpointDescription?: string;
}
