/**
 * Tool Event Builders
 * Provides builders for tool call events
 */

import { createBuilder, type BuildParams } from "./common.js";
import type {
  ToolCallStartedEvent,
  ToolCallCompletedEvent,
  ToolCallFailedEvent,
  ToolCallBlockedEvent,
  ToolAddedEvent,
  ToolVisibilityChangedEvent,
} from "@wf-agent/types";

// =============================================================================
// Tool Call Events
// =============================================================================

/**
 * Build tool call started event
 */
export const buildToolCallStartedEvent = createBuilder<ToolCallStartedEvent>("TOOL_CALL_STARTED");

/**
 * Build tool call completed event
 */
export const buildToolCallCompletedEvent =
  createBuilder<ToolCallCompletedEvent>("TOOL_CALL_COMPLETED");

/**
 * Build tool call failed event
 * Note: Manually constructed because the caller passes error: Error which is
 * converted to string. The createStringErrorBuilder behavior differs slightly
 * (lacks "Unknown error" fallback), so we keep this manual.
 */
export const buildToolCallFailedEvent = (
  params: Omit<BuildParams<ToolCallFailedEvent>, "error"> & {
    error: Error;
    workflowId?: string;
  },
): ToolCallFailedEvent =>
  ({
    type: "TOOL_CALL_FAILED",
    timestamp: Date.now(),
    ...params,
    error: params.error.message || "Unknown error",
  }) as ToolCallFailedEvent;

/**
 * Build tool call blocked event (NEW - for failure protection)
 */
export const buildToolCallBlockedEvent = createBuilder<ToolCallBlockedEvent>("TOOL_CALL_BLOCKED");

// =============================================================================
// Tool Events
// =============================================================================

/**
 * Build tool added event
 */
export const buildToolAddedEvent = createBuilder<ToolAddedEvent>("TOOL_ADDED");

/**
 * Build tool visibility changed event
 */
export const buildToolVisibilityChangedEvent =
  createBuilder<ToolVisibilityChangedEvent>("TOOL_VISIBILITY_CHANGED");
