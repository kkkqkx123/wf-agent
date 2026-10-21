/**
 * MockEventSource — fixture-backed SSE stream.
 *
 * `sse.ts` opens a real `EventSource` against `/api/v1/events/stream`, which
 * no preview backend serves. Swap in a fixture implementation that replays
 * the same frame contract: a `connected` handshake, checkpoint-domain events
 * published by the mutation helpers (approve / delete / create / …), and a
 * slow heartbeat so the live badge and incremental refresh stay observable.
 */

import { emitMockEvent, onMockEvent } from '#lib/fixtures/checkpoints';

/** Rotating heartbeat events so the Live subscription visibly fires. */
const HEARTBEAT_EVENTS = [
	'CHECKPOINT_FILE_CHANGED',
	'CHECKPOINT_CREATED',
	'TOOL_APPROVAL_REQUESTED',
];
const HEARTBEAT_INTERVAL_MS = 30_000;

class MockEventSource {
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSED = 2;

	readyState = MockEventSource.CONNECTING;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	onmessage: ((event: any) => void) | null = null;
	onerror: (() => void) | null = null;

	private unlisten: (() => void) | null = null;
	private heartbeat: ReturnType<typeof setInterval> | null = null;
	private beat = 0;

	constructor(_url: string) {
		void _url;
		// Deliver on a macrotask so the caller finishes assigning
		// the handlers before the handshake frame arrives.
		setTimeout(() => {
			this.readyState = MockEventSource.OPEN;
			this.onmessage?.({
				data: JSON.stringify({ type: 'connected', timestamp: Date.now() }),
			});
			this.unlisten = onMockEvent((frame) => {
				this.onmessage?.({ data: JSON.stringify(frame) });
			});
			this.heartbeat = setInterval(() => {
				emitMockEvent(HEARTBEAT_EVENTS[this.beat % HEARTBEAT_EVENTS.length]);
				this.beat += 1;
			}, HEARTBEAT_INTERVAL_MS);
		}, 50);
	}

	close(): void {
		this.readyState = MockEventSource.CLOSED;
		this.unlisten?.();
		this.unlisten = null;
		if (this.heartbeat !== null) clearInterval(this.heartbeat);
		this.heartbeat = null;
		this.onmessage = null;
		this.onerror = null;
	}
}

if (typeof window !== 'undefined') {
	(window as { EventSource?: unknown }).EventSource = MockEventSource;
}
