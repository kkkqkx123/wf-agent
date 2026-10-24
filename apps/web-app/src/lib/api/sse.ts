import { API_BASE_URL, client, resolveApiKey } from '#lib/api/client';
import { call } from '#lib/api/envelope';
import type { EventRecord } from '#lib/types/models';
import { decodeSystemFrame } from '#lib/utils/stream-parser';
import type { BaseEventFrame } from '#lib/utils/stream-parser';

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

function toEventRecord(frame: BaseEventFrame | StreamFrame): EventRecord {
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
	/** Only checkpoint-domain events (excludes execution noise). */
	checkpointOnly?: boolean;
	/** Narrow checkpoint events to one file path. */
	filePath?: string;
	/** Narrow checkpoint events to one snapshot or commit id. */
	snapshotId?: string;
	onEvent: (event: EventRecord) => void;
	onState: (state: StreamState) => void;
}

/**
 * Fetch a single-use ticket for the events stream. Returns undefined when the
 * endpoint is unavailable so callers fall back to the key query parameter.
 */
async function fetchStreamTicket(): Promise<string | undefined> {
	try {
		const data = await call<unknown>(
			client.POST('/api/v1/events/stream-ticket', {
				headers: { 'x-wf-ticket': '1' },
			}),
		);
		const ticket = (data as { ticket?: unknown })?.ticket;
		return typeof ticket === 'string' ? ticket : undefined;
	} catch {
		return undefined;
	}
}

/**
 * Open the filtered events SSE stream. The handshake authenticates with a
 * single-use ticket minted by a header-keyed request, so the persistent API
 * key never lands in the stream URL (EventSource cannot set headers).
 * Reconnects with `since=<last timestamp>`, minting a fresh ticket each time
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
		onState(attempt === 0 ? 'connecting' : 'reconnecting');
		// The handshake authenticates with a single-use ticket (the persistent
		// key stays in the minting request's header); fall back to the key
		// query parameter only when the endpoint is unavailable.
		void fetchStreamTicket().then((ticket) => {
			if (closed) return;
			const params = new URLSearchParams();
			if (options.executionId) params.set('execution_id', options.executionId);
			if (options.agentLoopId) params.set('agent_loop_id', options.agentLoopId);
			if (options.workflowId) params.set('workflow_id', options.workflowId);
			if (options.checkpointOnly) params.set('checkpoint_only', 'true');
			if (options.filePath) params.set('file_path', options.filePath);
			if (options.snapshotId) params.set('snapshot_id', options.snapshotId);
			if (since !== null) params.set('since', String(since));
			if (ticket) {
				params.set('ticket', ticket);
			} else {
				const key = resolveApiKey();
				if (key) params.set('api_key', key);
			}

			source = new EventSource(
				`${API_BASE_URL}/api/v1/events/stream?${params.toString()}`,
			);
			source.onmessage = (message) => {
				let raw: unknown;
				try {
					raw = JSON.parse(message.data as string) as unknown;
				} catch {
					return;
				}
				const decoded = decodeSystemFrame(raw);
				if (decoded.kind === 'connected') {
					attempt = 0;
					onState('open');
					return;
				}
				if (decoded.kind !== 'base') return;
				const frame = decoded.base;
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
		});
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
