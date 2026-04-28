/**
 * Hook Event Builders
 * Provides builders for hook-related events
 */

import { now } from "@wf-agent/common-utils";

// =============================================================================
// Hook Event Types (internal)
// =============================================================================

/**
 * Hook executed event (internal use)
 */
export interface HookExecutedEvent {
  type: "HOOK_EXECUTED";
  timestamp: number;
  /** Hook event name */
  eventName: string;
  /** Event data */
  data: Record<string, unknown>;
}

// =============================================================================
// Hook Event Builders
// =============================================================================

/**
 * Build hook executed event
 */
export const buildHookExecutedEvent = (
  params: Omit<HookExecutedEvent, "type" | "timestamp">,
): HookExecutedEvent => ({
  type: "HOOK_EXECUTED",
  timestamp: now(),
  ...params,
});
