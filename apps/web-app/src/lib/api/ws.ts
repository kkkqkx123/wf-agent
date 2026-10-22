import { API_BASE_URL, resolveApiKey } from '#lib/api/client';
import type { EventRecord } from '#lib/types/models';
import type { StreamState } from '#lib/api/sse';

export type WsSubscription =
	| { executionId: string; since?: string | null }
	| { agentLoopId: string; since?: string | null }
	| { workflowId: string; since?: string | null }
	| { global: true; since?: string | null }
	| { notifications: true; since?: string | null }
	| { checkpoint: true; filePath?: string; snapshotId?: string; since?: string | null };

export interface WsEventOptions {
	subscriptions: WsSubscription[];
	onEvent: (event: EventRecord) => void;
	onState: (state: StreamState) => void;
}

interface WsWireEvent {
	type?: string;
	executionId?: string;
	agentLoopId?: string;
	workflowId?: string;
	eventType?: string;
	data?: Record<string, unknown>;
	timestamp?: number;
	cursor?: string;
}

function subscriptionMessage(sub: WsSubscription, since: string | null): Record<string, unknown> {
	const base: Record<string, unknown> = { type: 'subscribe' };
	if ('executionId' in sub) base.executionId = sub.executionId;
	else if ('agentLoopId' in sub) base.agentLoopId = sub.agentLoopId;
	else if ('workflowId' in sub) base.workflowId = sub.workflowId;
	else if ('global' in sub) base.global = true;
	else if ('notifications' in sub) base.notifications = true;
	else if ('checkpoint' in sub) {
		base.checkpoint = true;
		if (sub.filePath) base.filePath = sub.filePath;
		if (sub.snapshotId) base.snapshotId = sub.snapshotId;
	}
	const explicit = 'since' in sub ? (sub as { since?: string | null }).since : undefined;
	const effective = explicit ?? since;
	if (effective) base.since = effective;
	return base;
}

function toEventRecord(wire: WsWireEvent): EventRecord | null {
	if (!wire.eventType) return null;
	const data = wire.data ?? {};
	const raw = data as Record<string, unknown>;
	const executionId =
		typeof wire.executionId === 'string'
			? wire.executionId
			: typeof wire.agentLoopId === 'string'
				? wire.agentLoopId
				: typeof wire.workflowId === 'string'
					? wire.workflowId
					: typeof raw.execution_id === 'string'
						? raw.execution_id
						: typeof raw.agent_loop_id === 'string'
							? raw.agent_loop_id
							: null;
	const at =
		typeof wire.timestamp === 'number' ? new Date(wire.timestamp).toISOString() : '';
	return {
		id: typeof wire.cursor === 'string' ? wire.cursor : '',
		type: wire.eventType,
		source: 'stream',
		at,
		executionId,
		payload: JSON.stringify(data),
	};
}

function wsUrl(): string {
	const key = resolveApiKey();
	const base = API_BASE_URL.replace(/^http/, 'ws');
	const params = new URLSearchParams();
	if (key) params.set('api_key', key);
	const suffix = params.toString();
	return `${base}/api/v1/ws${suffix ? `?${suffix}` : ''}`;
}

/**
 * Open a multi-dimensional WebSocket subscription. Authentication reuses the
 * `api_key` query parameter because browsers cannot set WS headers; the
 * events-stream ticket scope does not cover `/api/v1/ws`.
 * Reconnects with exponential backoff, resubscribes every dimension and
 * replays from the last cursor so bounded server history fills the gap.
 */
export function openWebSocket(options: WsEventOptions): () => void {
	const { subscriptions, onEvent, onState } = options;
	let socket: WebSocket | null = null;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let pingTimer: ReturnType<typeof setInterval> | null = null;
	let lastCursor: string | null = null;
	let lastTimestamp = 0;
	let attempt = 0;
	let closed = false;

	function sendSubscriptions(): void {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		for (const sub of subscriptions) {
			socket.send(JSON.stringify(subscriptionMessage(sub, lastCursor)));
		}
	}

	function connect(): void {
		if (closed) return;
		onState(attempt === 0 ? 'connecting' : 'reconnecting');
		let next: WebSocket;
		try {
			next = new WebSocket(wsUrl());
		} catch {
			scheduleReconnect();
			return;
		}
		socket = next;

		socket.onopen = () => {
			attempt = 0;
			onState('open');
			sendSubscriptions();
			if (pingTimer !== null) clearInterval(pingTimer);
			pingTimer = setInterval(() => {
				if (socket?.readyState === WebSocket.OPEN) {
					try {
						socket.send(JSON.stringify({ type: 'ping' }));
					} catch {
						// A failed ping surfaces through onclose/onerror.
					}
				}
			}, 25_000);
		};

		socket.onmessage = (message: MessageEvent) => {
			let raw: unknown;
			try {
				raw = JSON.parse(message.data as string) as unknown;
			} catch {
				return;
			}
			const wire = raw as WsWireEvent;
			if (!wire || typeof wire.type !== 'string') return;
			if (wire.type === 'pong' || wire.type === 'connection' || wire.type === 'subscribed' || wire.type === 'unsubscribed') {
				if (wire.type === 'subscribed') {
					attempt = 0;
					onState('open');
				}
				return;
			}
			if (wire.type === 'error') return;
			const record = toEventRecord(wire);
			if (!record) return;
			if (typeof wire.cursor === 'string' && wire.cursor) lastCursor = wire.cursor;
			if (typeof wire.timestamp === 'number') lastTimestamp = wire.timestamp;
			void lastTimestamp;
			onEvent(record);
		};

		const onClosed = (): void => {
			if (pingTimer !== null) {
				clearInterval(pingTimer);
				pingTimer = null;
			}
			if (closed) return;
			scheduleReconnect();
		};

		socket.onclose = onClosed;
		socket.onerror = () => {
			try {
				socket?.close();
			} catch {
				// Close errors surface through onclose.
			}
		};
	}

	function scheduleReconnect(): void {
		if (closed) return;
		attempt += 1;
		const delay = Math.min(1000 * 2 ** attempt, 10_000);
		onState('reconnecting');
		if (timer !== null) clearTimeout(timer);
		timer = setTimeout(() => {
			if (!closed) connect();
		}, delay);
	}

	connect();

	return () => {
		closed = true;
		if (timer !== null) clearTimeout(timer);
		if (pingTimer !== null) clearInterval(pingTimer);
		try {
			socket?.close();
		} catch {
			// Closing a broken socket is harmless.
		}
		socket = null;
		onState('closed');
	};
}
