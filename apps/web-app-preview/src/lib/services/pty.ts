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
	title?: string;
}): Promise<PtySession> {
	const data = await call<unknown>(
		client.POST('/api/v1/pty', {
			body: {
				cwd: input?.cwd,
				command: input?.command,
				rows: input?.rows,
				cols: input?.cols,
				title: input?.title,
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

export async function updatePty(
	id: string,
	input: { title?: string; rows?: number; cols?: number },
): Promise<PtySession> {
	const data = await call<unknown>(
		client.PUT('/api/v1/pty/{id}', {
			params: { path: { id } },
			body: {
				title: input.title,
				rows: input.rows,
				cols: input.cols,
			},
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
	/** Current underlying socket, null while disconnected. */
	socket(): WebSocket | null;
	/** Send user input to the session. Queued before the socket opens. */
	send(data: string | ArrayBuffer | Uint8Array): void;
	/** Close the socket and never reopen it. */
	dispose(): void;
}

export interface PtySocketOptions {
	/** Called after the server replay frame arrives; cursor starts at `value`. */
	onMeta?: (cursor: number) => void;
	/** Called with raw terminal output chunks. */
	onData?: (chunk: string) => void;
	/** Called when the server reports the session exited (control frame). */
	onExit?: (info: { exitCode: number | null; reason: string }) => void;
	/** Called with the close code whenever the socket leaves the open state. */
	onClose?: (code: number, reason: string) => void;
}

/**
 * Fetch a single-use connect ticket for the session. The persistent API key
 * travels in the request header, never in the websocket URL. Returns
 * undefined when the endpoint is unavailable so the caller can fall back.
 */
async function fetchConnectTicket(id: string): Promise<string | undefined> {
	try {
		const data = await call<unknown>(
			client.POST('/api/v1/pty/{id}/connect-token', {
				params: { path: { id } },
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
 * Open the PTY websocket for a session. The handshake authenticates with a
 * single-use connect ticket (the resume cursor tells the server what to
 * replay). Reconnect policy is the caller's decision, driven by `onClose`;
 * every (re)connect mints a fresh ticket because tickets are one-time.
 */
export function connectPty(
	id: string,
	cursor: number,
	options: PtySocketOptions = {},
): PtySocket {
	let ws: WebSocket | null = null;
	let disposed = false;
	let pending: Array<string | ArrayBuffer | Uint8Array> = [];

	function flushPending(target: WebSocket): void {
		if (pending.length === 0) return;
		const items = pending;
		pending = [];
		for (let i = 0; i < items.length; i += 1) {
			if (disposed) {
				pending = items.slice(i);
				return;
			}
			if (target.readyState !== WebSocket.OPEN) {
				pending = items.slice(i);
				return;
			}
			try {
				target.send(items[i] as string);
			} catch {
				pending = items.slice(i);
				return;
			}
		}
	}

	function send(data: string | ArrayBuffer | Uint8Array): void {
		if (disposed) return;
		if (!data) return;
		if (typeof data === 'string' && data.length === 0) return;
		if (data instanceof Uint8Array && data.length === 0) return;
		if (data instanceof ArrayBuffer && data.byteLength === 0) return;
		const current = ws;
		if (current && current.readyState === WebSocket.OPEN) {
			try {
				current.send(data as string);
			} catch {
				// Keep unsent input for the next connection.
				if (pending.length < 200) pending.push(data);
			}
			return;
		}
		if (pending.length < 200) pending.push(data);
	}

	void fetchConnectTicket(id).then((ticket) => {
		if (disposed) return;
		const url = new URL(`${API_BASE_URL}/api/v1/pty/${id}/connect`);
		if (ticket) {
			url.searchParams.set('ticket', ticket);
		} else {
			const key = resolveApiKey();
			if (key) url.searchParams.set('api_key', key);
		}
		url.searchParams.set('cursor', String(cursor));
		url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';

		const socket = new WebSocket(url.toString());
		ws = socket;
		socket.binaryType = 'arraybuffer';

		socket.onopen = () => {
			if (disposed) return;
			flushPending(socket);
		};

		socket.onmessage = (event: MessageEvent) => {
			if (disposed) return;
			if (event.data instanceof ArrayBuffer) {
				const bytes = new Uint8Array(event.data);
				if (bytes.length === 0 || bytes[0] !== 0x00) return;
				try {
					const meta = JSON.parse(
						new TextDecoder().decode(bytes.subarray(1)),
					) as {
						cursor?: unknown;
						event?: unknown;
						exit_code?: unknown;
						reason?: unknown;
					};
					if (meta.event === 'exited') {
						options.onExit?.({
							exitCode:
								typeof meta.exit_code === 'number'
									? meta.exit_code
									: null,
							reason:
								typeof meta.reason === 'string'
									? meta.reason
									: 'process',
						});
						return;
					}
					const next = meta?.cursor;
					if (
						typeof next === 'number' &&
						Number.isSafeInteger(next) &&
						next >= 0
					) {
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
			options.onClose?.(event.code, event.reason);
		};
	});

	return {
		socket() {
			return ws;
		},
		send(data: string | ArrayBuffer | Uint8Array): void {
			send(data);
		},
		dispose() {
			disposed = true;
			pending = [];
			if (ws && ws.readyState <= WebSocket.OPEN) {
				ws.close(1000);
			}
			ws = null;
		},
	};
}

/**
 * Merge rapid output chunks into single writes so large bursts do not
 * overwhelm the xterm render loop. Flush is scheduled on a microtask.
 * Completion callbacks queue until pending writes settle.
 */
export function createTerminalWriter(
	write: (data: string, done?: () => void) => void,
	schedule: (flush: () => void) => void = queueMicrotask,
): { push(data: string): void; flush(done?: () => void): void } {
	let chunks: string[] | undefined;
	let waits: Array<() => void> | undefined;
	let scheduled = false;
	let writing = false;

	const settle = () => {
		if (scheduled || writing || chunks?.length) return;
		const list = waits;
		if (!list?.length) return;
		waits = undefined;
		for (const fn of list) fn();
	};

	const run = () => {
		if (writing) return;
		scheduled = false;
		const items = chunks;
		if (!items?.length) {
			settle();
			return;
		}
		chunks = undefined;
		writing = true;
		write(items.join(''), () => {
			writing = false;
			if (chunks?.length) {
				if (scheduled) return;
				scheduled = true;
				schedule(run);
				return;
			}
			settle();
		});
	};

	return {
		push(data: string) {
			if (!data) return;
			if (chunks) chunks.push(data);
			else chunks = [data];
			if (scheduled || writing) return;
			scheduled = true;
			schedule(run);
		},
		flush(done?: () => void) {
			if (!scheduled && !writing && !chunks?.length) {
				done?.();
				return;
			}
			if (done) {
				if (waits) waits.push(done);
				else waits = [done];
			}
			run();
		},
	};
}
