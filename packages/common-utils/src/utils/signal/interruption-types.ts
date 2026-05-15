/**
 * Interrupt Result Type Definition
 * Handling control flow interrupts using the return value marker system instead of the error system
 *
 * @deprecated These types have been moved to SDK internal implementation.
 * Use sdk/core/types/interruption-types.ts and sdk/core/utils/interruption/ instead.
 */

import type { InterruptionCheckResult } from "./abort-signal-utils.js";

/**
 * interrupt message
 */
export interface InterruptionInfo {
  type: "PAUSE" | "STOP";
  executionId: string;
  nodeId: string;
  timestamp?: number;
}

/**
 * Configuration for interruptible operation
 */
export interface InterruptibleOptions {
  /** Customized Check Functions */
  customCheck?: () => InterruptionCheckResult;
}
