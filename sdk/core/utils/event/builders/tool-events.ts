/**
 * Tool Event Builders
 * Provides builders for tool call events
 */

import { createBuilder, createStringErrorBuilder } from "./common.js";
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
 * Converts Error to string with "Unknown error" fallback
 */
export const buildToolCallFailedEvent =
  createStringErrorBuilder<ToolCallFailedEvent>("TOOL_CALL_FAILED");

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
