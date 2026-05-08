/**
 * Interrupt Result Type Definition
 * Handling control flow interrupts using the return value marker system instead of the error system
 */

import type { InterruptionType } from "@wf-agent/types";

/**
 * interrupt message
 */
export interface InterruptionInfo {
  type: Exclude<InterruptionType, null>;
  executionId: string;
  nodeId: string;
  timestamp?: number;
}

/**
 * Configuration for interruptible operation
 */
export interface InterruptibleOptions {
  /** Check interval (milliseconds), default 0 (checked on every call) */
  checkInterval?: number;
  /** Customized Check Functions */
  customCheck?: () => import("./abort-signal-utils.js").InterruptionCheckResult;
}
