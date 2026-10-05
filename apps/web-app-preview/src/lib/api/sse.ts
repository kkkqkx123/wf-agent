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

const MOCK_EVENT_TYPES = [
	'started',
	'iteration_start',
	'tool_call',
	'tool_result',
	'iteration_end',
	'completed',
	'failed',
] as const;

let mockEventCounter = 0;

function createMockFrame(timestamp: number): StreamFrame {
	const type = MOCK_EVENT_TYPES[mockEventCounter % MOCK_EVENT_TYPES.length]!;
	mockEventCounter += 1;
	return {
		id: `mock-${mockEventCounter}`,
		type,
		timestamp,
		metadata: {
			sequence: mockEventCounter,
			source: 'preview-mock',
		},
	};
}

/**
 * Open the filtered events SSE stream. In preview mode (API_BASE_URL === '/mock'),
 * this simulates a local event stream instead of connecting to a real server,
 * avoiding infinite reconnection loops against a non-existent endpoint.
 */
export function openEventStream(options: EventStreamOptions): () => void {
	const { onEvent, onState } = options;
	let closed = false;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let lastTimestamp = 0;

	if (API_BASE_URL === '/mock') {
		onState('connecting');
		const handshake = setTimeout(() => {
			if (closed) return;
			onState('open');
			timer = setInterval(() => {
				if (closed) return;
				const frame = createMockFrame(Date.now());
				lastTimestamp = frame.timestamp ?? 0;
				onEvent(toEventRecord(frame));
			}, 2000);
		}, 100);

		return () => {
			closed = true;
			clearTimeout(handshake);
			if (timer !== null) clearInterval(timer);
			onState('closed');
		};
	}

	let source: EventSource | null = null;
	let attempt = 0;

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
