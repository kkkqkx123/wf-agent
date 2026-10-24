import { openRealtime } from '#lib/api/realtime';
import type { StreamState } from '#lib/api/sse';
import type { EventRecord } from '#lib/types/models';

/** Bus events that invalidate execution/trigger list segments. */
const LIST_DIRTY_TYPES = new Set([
	'WORKFLOW_EXECUTION_STARTED',
	'WORKFLOW_EXECUTION_COMPLETED',
	'WORKFLOW_EXECUTION_FAILED',
	'WORKFLOW_EXECUTION_CANCELLED',
	'WORKFLOW_EXECUTION_PAUSED',
	'WORKFLOW_EXECUTION_RESUMED',
	'AGENT_STARTED',
	'AGENT_COMPLETED',
	'AGENT_FAILED',
	'AGENT_CANCELLED',
	'AGENT_PAUSED',
	'AGENT_RESUMED',
	'TOOL_APPROVAL_REQUESTED',
	'TOOL_APPROVAL_RESPONDED',
	'TOOL_APPROVAL_FAILED',
	'FOLLOWUP_QUESTION_REQUESTED',
	'FOLLOWUP_QUESTION_RESPONDED',
	'FOLLOWUP_QUESTION_FAILED',
	'NOTIFICATION_SENT',
	'ERROR',
	'TRIGGERED_SUBGRAPH_STARTED',
	'TRIGGERED_SUBGRAPH_COMPLETED',
	'TRIGGERED_SUBGRAPH_FAILED',
]);

export function isListDirtyEvent(type: string): boolean {
	return LIST_DIRTY_TYPES.has(type.trim().toUpperCase());
}

export interface ListInvalidationOptions {
	onDirty: (event: EventRecord) => void;
	onState?: (state: StreamState) => void;
}

/**
 * Shared WS fan-in for list pages: one `global` plus one `notifications`
 * subscription drives dirty flags. Payloads stay metadata-only; pages keep
 * REST for rows and pagination, debounce reloads and only auto-reload when
 * visible and idle.
 */
export function watchListInvalidation(
	options: ListInvalidationOptions,
): () => void {
	const { onDirty, onState } = options;
	let pending: EventRecord | null = null;
	let timer: ReturnType<typeof setTimeout> | null = null;

	const flush = (): void => {
		timer = null;
		const event = pending;
		pending = null;
		if (event) onDirty(event);
	};

	return openRealtime({
		dimensions: [{ global: true }, { notifications: true }],
		onEvent: (event) => {
			if (!isListDirtyEvent(event.type)) return;
			pending = event;
			if (timer === null) timer = setTimeout(flush, 400);
		},
		onState: (state) => onState?.(state),
	});
}
