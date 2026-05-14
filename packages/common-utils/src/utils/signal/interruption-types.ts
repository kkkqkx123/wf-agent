/**
 * Interrupt Result Type Definition
 * Handling control flow interrupts using the return value marker system instead of the error system
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
