import { openRealtime } from '#lib/api/realtime';
import type { StreamState } from '#lib/api/sse';
import type { EventRecord } from '#lib/types/models';

/** Execution statuses that still change without intervention. */
const LIVE_STATUSES = new Set([
	'running',
	'in_progress',
	'executing',
	'streaming',
	'started',
	'pending',
]);

export function isLiveStatus(status: string): boolean {
	return LIVE_STATUSES.has(status.trim().toLowerCase());
}

/** Lifecycle event types after which an execution no longer changes. */
const TERMINAL_EVENT_TYPES = new Set([
	'WORKFLOW_EXECUTION_COMPLETED',
	'WORKFLOW_EXECUTION_FAILED',
	'WORKFLOW_EXECUTION_CANCELLED',
	'AGENT_COMPLETED',
	'AGENT_FAILED',
	'AGENT_CANCELLED',
]);

export function isTerminalEventType(type: string): boolean {
	return TERMINAL_EVENT_TYPES.has(type.trim().toUpperCase());
}

/** Interaction lifecycle: approvals and follow-up questions plus run ends. */
const INTERACTION_EVENT_TYPES = new Set([
	'TOOL_APPROVAL_REQUESTED',
	'TOOL_APPROVAL_RESPONDED',
	'TOOL_APPROVAL_FAILED',
	'FOLLOWUP_QUESTION_REQUESTED',
	'FOLLOWUP_QUESTION_RESPONDED',
	'FOLLOWUP_QUESTION_FAILED',
	'AGENT_COMPLETED',
	'AGENT_FAILED',
	'AGENT_CANCELLED',
]);

export function isInteractionEventType(type: string): boolean {
	return INTERACTION_EVENT_TYPES.has(type.trim().toUpperCase());
}

export interface ActivityCallbacks {
	onEvent?: (event: EventRecord) => void;
	onTerminal?: (event: EventRecord) => void;
	onState?: (state: StreamState) => void;
}

/**
 * Watch one execution for activity. Every live event reaches `onEvent`;
 * the first terminal event additionally reaches `onTerminal`, after which
 * the caller typically reloads the execution detail. The stream stays
 * open: a terminal event is observed, not assumed to end the transport.
 * Single-execution fan-in stays on SSE through the unified realtime
 * client; multi-dimensional pages use the same client and upgrade to WS.
 */
export function watchExecutionActivity(
	executionId: string,
	callbacks: ActivityCallbacks,
): () => void {
	return openRealtime({
		dimensions: [{ executionId }],
		onEvent: (event) => {
			callbacks.onEvent?.(event);
			if (isTerminalEventType(event.type)) {
				callbacks.onTerminal?.(event);
			}
		},
		onState: (state) => callbacks.onState?.(state),
	});
}
