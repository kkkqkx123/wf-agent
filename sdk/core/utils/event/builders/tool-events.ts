/**
 * Tool Event Builders
 * Provides builders for tool call events
 */

import { now } from "@wf-agent/common-utils";
import { createBuilder, type BuildParams } from "./common.js";
import type {
  ToolCallStartedEvent,
  ToolCallCompletedEvent,
  ToolCallFailedEvent,
  ToolAddedEvent,
} from "@wf-agent/types";

// =============================================================================
// Tool Call Events
// =============================================================================

/**
 * Build tool call started event
 */
export const buildToolCallStartedEvent = (
  params: BuildParams<ToolCallStartedEvent> & { workflowId?: string },
): ToolCallStartedEvent =>
  ({ type: "TOOL_CALL_STARTED", timestamp: now(), ...params }) as ToolCallStartedEvent;

/**
 * Build tool call completed event
 */
export const buildToolCallCompletedEvent = (
  params: BuildParams<ToolCallCompletedEvent> & { workflowId?: string },
): ToolCallCompletedEvent =>
  ({ type: "TOOL_CALL_COMPLETED", timestamp: now(), ...params }) as ToolCallCompletedEvent;

/**
 * Build tool call failed event
 */
export const buildToolCallFailedEvent = (
  params: Omit<BuildParams<ToolCallFailedEvent>, "error"> & { error: Error; workflowId?: string },
): ToolCallFailedEvent =>
  ({
    type: "TOOL_CALL_FAILED",
    timestamp: now(),
    ...params,
    error: params.error.message || "Unknown error",
  }) as ToolCallFailedEvent;

// =============================================================================
// Tool Events
// =============================================================================

/**
 * Build tool added event
 */
export const buildToolAddedEvent = createBuilder<ToolAddedEvent>("TOOL_ADDED");
