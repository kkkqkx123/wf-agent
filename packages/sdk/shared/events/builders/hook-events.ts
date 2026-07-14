/**
 * Hook Event Builders
 * Provides builders for hook-related events
 *
 * @remarks All types and builders in this file are for internal use only.
 * HookExecutedEvent uses manual construction because "HOOK_EXECUTED" is not part of EventType union.
 */

import { now, generateId } from "@wf-agent/common-utils";

// =============================================================================
// Hook Event Types (internal)
// =============================================================================

/**
 * Hook executed event (internal use, not part of public Event type union)
 */
export interface HookExecutedEvent {
  type: "HOOK_EXECUTED";
  id: string;
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
  params: Omit<HookExecutedEvent, "type" | "timestamp" | "id">,
): HookExecutedEvent => ({
  id: generateId(),
  type: "HOOK_EXECUTED",
  timestamp: now(),
  ...params,
});