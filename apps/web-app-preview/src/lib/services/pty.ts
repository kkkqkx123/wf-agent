import {
	API_BASE_URL,
	client,
	resolveApiKey,
} from '$lib/api/client';
import { call, requireData } from '$lib/api/envelope';
import type { components } from '$lib/api/schema';

export type PtySession = components['schemas']['PtySession'];

export type PtySessionInfo = PtySession;

/**
 * Create a PTY session. When `command` is omitted the server spawns the
 * user's login shell.
 */
export async function createPty(input?: {
	cwd?: string;
	command?: string;
	rows?: number;
	cols?: number;
}): Promise<PtySession> {
	const data = await call<unknown>(
		client.POST('/api/v1/pty', {
			body: {
				cwd: input?.cwd,
				command: input?.command,
				rows: input?.rows,
				cols: input?.cols,
			},
		}),
	);
	return requireData(data, 'PTY session') as PtySession;
}

export async function listPtys(): Promise<PtySession[]> {
	const data = await call<unknown>(client.GET('/api/v1/pty'));
	const rows = requireData(data, 'PTY sessions') as PtySession[];
	return Array.isArray(rows) ? rows : [];
}

export async function getPty(id: string): Promise<PtySession> {
	const data = await call<unknown>(
		client.GET('/api/v1/pty/{id}', { params: { path: { id } } }),
	);
	return requireData(data, 'PTY session') as PtySession;
}

export async function resizePty(
	id: string,
	rows: number,
	cols: number,
): Promise<PtySession> {
	const data = await call<unknown>(
		client.POST('/api/v1/pty/{id}/resize', {
			params: { path: { id } },
			body: { rows, cols },
		}),
	);
	return requireData(data, 'PTY session') as PtySession;
}

export async function closePty(id: string): Promise<void> {
	await call<unknown>(
		client.DELETE('/api/v1/pty/{id}', { params: { path: { id } } }),
	);
}

export interface PtySocket {
	/** Close the websocket (sends code 1000). */
	close(): void;
	/** Current underlying socket, null while disconnected. */
	socket(): WebSocket | null;
	/** Terminate the socket and do not reconnect. */
	dispose(): void;
	/** Register a callback fired with the close code on abnormal disconnect. */
	onClose(listener: (code: number) => void): void;
}

export interface PtySocketOptions {
	/** Reconnect with exponential backoff after abnormal close. */
	reconnect?: boolean;
	/** Called after the server replay frame arrives; cursor starts at `value`. */
	onMeta?: (cursor: number) => void;
	/** Called with raw terminal output chunks. */
	onData?: (chunk: string) => void;
	/** Called when the server closes the stream (session exited or removed). */
	onServerClose?: (code: number) => void;
}

/**
 * Open the PTY websocket for a session. The URL carries the API key as a
 * query parameter (browsers cannot set WebSocket headers) and the resume
 * cursor so the server replays everything after it.
 */
export function connectPty(
	id: string,
	cursor: number,
	options: PtySocketOptions = {},
): PtySocket {
	const key = resolveApiKey();
	const base = API_BASE_URL;
	const url = new URL(`${base}/api/v1/pty/${id}/connect`);
	if (key) url.searchParams.set('api_key', key);
	url.searchParams.set('cursor', String(cursor));
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';

	let ws: WebSocket | null = null;
	let disposed = false;
	let tries = 0;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let listeners: Array<(code: number) => void> = [];
	let gotMeta = false;

	const clearTimer = () => {
		if (timer !== undefined) {
			clearTimeout(timer);
			timer = undefined;
		}
	};

	const open = () => {
		if (disposed) return;
		const socket = new WebSocket(url.toString());
		ws = socket;
		socket.binaryType = 'arraybuffer';

		socket.onmessage = (event: MessageEvent) => {
			if (disposed) return;
			if (event.data instanceof ArrayBuffer) {
				const bytes = new Uint8Array(event.data);
				if (bytes.length === 0 || bytes[0] !== 0x00) return;
				try {
					const meta = JSON.parse(
						new TextDecoder().decode(bytes.subarray(1)),
					) as { cursor?: unknown };
					const next = meta?.cursor;
					if (
						typeof next === 'number' &&
						Number.isSafeInteger(next) &&
						next >= 0
					) {
						gotMeta = true;
						options.onMeta?.(next);
					}
				} catch {
					// Ignore malformed control frames.
				}
				return;
			}
			const chunk = typeof event.data === 'string' ? event.data : '';
			if (!chunk) return;
			options.onData?.(chunk);
		};

		socket.onclose = (event: CloseEvent) => {
			if (disposed) return;
			ws = null;
			if (event.code === 1000) return;
			clearTimer();
			if (!options.reconnect) return;
			// Never reconnect after the session is gone: replay only carries
			// output while the server still holds the session.
			if (gotMeta && event.code === 4404) {
				options.onServerClose?.(event.code);
				return;
			}
			const delay = Math.min(250 * 2 ** tries, 4000);
			tries += 1;
			timer = setTimeout(open, delay);
		};
	};

	open();

	return {
		close() {
			disposed = true;
			clearTimer();
			if (ws && ws.readyState <= WebSocket.OPEN) {
				ws.close(1000);
			}
			ws = null;
		},
		socket() {
			return ws;
		},
		dispose() {
			disposed = true;
			clearTimer();
			listeners = [];
			if (ws && ws.readyState <= WebSocket.OPEN) {
				ws.close(1000);
			}
			ws = null;
		},
		onClose(listener) {
			listeners.push(listener);
		},
	};
}

/**
 * Merge rapid output chunks into single writes so large bursts do not
 * overwhelm the xterm render loop. Flush is scheduled on a microtask.
 */
export function createTerminalWriter(
	write: (data: string) => void,
): { push(data: string): void; flush(): void } {
	let chunks: string[] | undefined;
	let scheduled = false;
	let writing = false;

	const run = () => {
		if (writing) return;
		scheduled = false;
		const items = chunks;
		if (!items?.length) return;
		chunks = [];
		writing = true;
		try {
			write(items.join(''));
		} finally {
			writing = false;
			// `write` may have enqueued re-entrant pushes while running.
			const pending = chunks;
			if (pending && pending.length > 0) {
				scheduled = true;
				queueMicrotask(run);
			}
		}
	};

	return {
		push(data: string) {
			if (!data) return;
			if (chunks) chunks.push(data);
			else chunks = [data];
			if (scheduled || writing) return;
			scheduled = true;
			queueMicrotask(run);
		},
		flush() {
			run();
		},
	};
}
