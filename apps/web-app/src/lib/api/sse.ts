import { API_BASE_URL, resolveApiKey } from '$lib/api/client';
import type { EventRecord } from '$lib/types/models';

export type StreamState = 'connecting' | 'open' | 'reconnecting' | 'closed';

/** Wire shape of a BaseEvent frame from GET /events/stream. */
interface StreamFrame {
	id?: string;
	type?: string;
	timestamp?: number;
	event_name?: string;
	workflow_id?: string;
	execution_id?: string;
	agent_loop_id?: string;
	metadata?: Record<string, unknown>;
}

function toEventRecord(frame: StreamFrame): EventRecord {
	return {
		id: frame.id ?? '',
		type: frame.type ?? '',
		source: 'stream',
		at:
			typeof frame.timestamp === 'number'
				? new Date(frame.timestamp).toISOString()
				: '',
		executionId: frame.execution_id ?? frame.agent_loop_id ?? null,
		payload: frame.metadata ? JSON.stringify(frame.metadata) : '',
	};
}

export interface EventStreamOptions {
	executionId?: string;
	agentLoopId?: string;
	workflowId?: string;
	onEvent: (event: EventRecord) => void;
	onState: (state: StreamState) => void;
}

/**
 * Open the filtered events SSE stream. The server authenticates via the
 * `api_key` query parameter (EventSource cannot set headers), matching the
 * wf-server query-param auth mode. Reconnects with `since=<last timestamp>`
 * so the bounded server-side backlog fills any gap.
 */
export function openEventStream(options: EventStreamOptions): () => void {
	const { onEvent, onState } = options;
	let source: EventSource | null = null;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let lastTimestamp = 0;
	let attempt = 0;
	let closed = false;

	function connect(since: number | null): void {
		const params = new URLSearchParams();
		if (options.executionId) params.set('execution_id', options.executionId);
		if (options.agentLoopId) params.set('agent_loop_id', options.agentLoopId);
		if (options.workflowId) params.set('workflow_id', options.workflowId);
		if (since !== null) params.set('since', String(since));
		const key = resolveApiKey();
		if (key) params.set('api_key', key);

		onState(attempt === 0 ? 'connecting' : 'reconnecting');
		source = new EventSource(
			`${API_BASE_URL}/api/v1/events/stream?${params.toString()}`,
		);
		source.onmessage = (message) => {
			let frame: StreamFrame;
			try {
				frame = JSON.parse(message.data as string) as StreamFrame;
			} catch {
				return;
			}
			if (frame.type === 'connected') {
				attempt = 0;
				onState('open');
				return;
			}
			if (typeof frame.timestamp === 'number') {
				lastTimestamp = frame.timestamp;
			}
			onEvent(toEventRecord(frame));
		};
		source.onerror = () => {
			if (closed) return;
			source?.close();
			source = null;
			attempt += 1;
			const delay = Math.min(1000 * 2 ** attempt, 10_000);
			onState('reconnecting');
			timer = setTimeout(() => {
				if (!closed) connect(lastTimestamp || null);
			}, delay);
		};
	}

	connect(null);

	return () => {
		closed = true;
		if (timer !== null) clearTimeout(timer);
		source?.close();
		source = null;
		onState('closed');
	};
}
