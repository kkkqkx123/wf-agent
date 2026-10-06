import { openEventStream, type StreamState } from '$lib/api/sse';
import type { EventRecord } from '$lib/types/models';

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
 */
export function watchExecutionActivity(
	executionId: string,
	callbacks: ActivityCallbacks,
): () => void {
	return openEventStream({
		executionId,
		onEvent: (event) => {
			callbacks.onEvent?.(event);
			if (isTerminalEventType(event.type)) {
				callbacks.onTerminal?.(event);
			}
		},
		onState: (state) => callbacks.onState?.(state),
	});
}
