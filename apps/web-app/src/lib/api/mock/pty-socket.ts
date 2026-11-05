/**
 * Fixture-backed WebSocket for the PTY terminal in preview mode.
 *
 * `connectPty` opens a real WebSocket against `${API_BASE_URL}/api/v1/pty/{id}/connect`,
 * which no preview backend serves. Install a selective override: URLs pointing
 * at the mock base get an in-memory socket backed by a tiny scripted shell,
 * so the preview terminal stays interactive and the "type → output → scroll"
 * interaction chain can be exercised; every other URL keeps the native
 * WebSocket so real transports are untouched.
 *
 * The shell is deliberately minimal: it exists to produce believable,
 * streamed terminal output (chunked + delayed, like a real PTY) that
 * reflects the shared preview workspace (`workspace.ts`) — `ls` / `cat`
 * show what the IDE shows, `clear` emits ANSI, `exit` ends the session.
 * It is not a command interpreter; unknown input gets a not-found line.
 */

import { allFiles, listDir } from './workspace';
import { exitSession, getSession } from './pty-sessions';

const MOCK_URL_MARKER = '/api/v1/pty/';

const PROMPT = '\x1b[36mpreview\x1b[0m:\x1b[34m~/wf-agent\x1b[0m$ ';
const WELCOME = [
	'wf-agent preview shell — simulated PTY for UI verification.',
	"Type 'help' to list the supported commands.",
].join('\r\n');
const HELP = [
	'Supported preview commands:',
	'  help            show this help',
	'  ls [path]       list workspace entries (shared with the IDE)',
	'  cat <file>      print a workspace file',
	'  pwd             print the working directory',
	'  echo <text>     echo text back',
	'  whoami          print the preview user',
	'  clear           clear the terminal (ANSI)',
	'  exit            end the session',
].join('\r\n');

const CWD = '/preview/wf-agent';

/** Normalise a user-supplied path against the fixed preview cwd. */
function resolvePath(raw: string): string {
	const trimmed = raw.trim().replace(/^["']|["']$/g, '');
	if (!trimmed) return CWD;
	const parts: string[] = [];
	const base = trimmed.startsWith('/') ? '' : CWD;
	for (const part of `${base}/${trimmed}`.split('/')) {
		if (!part || part === '.') continue;
		if (part === '..') parts.pop();
		else parts.push(part);
	}
	return parts.join('/');
}

const OUTPUT_CHUNK_MS = 15;

function chunkText(text: string): string[] {
	// Split on code points so astral characters never tear across chunks.
	const points = [...text];
	const chunks: string[] = [];
	for (let i = 0; i < points.length; i += 64) {
		chunks.push(points.slice(i, i + 64).join(''));
	}
	return chunks;
}

function listOutput(arg: string): string {
	const path = resolvePath(arg);
	const rows = listDir(path === CWD ? '' : path.replace(/^\/preview\/wf-agent\/?/, ''));
	if (rows.length === 0) return `(empty) ${path}`;
	return rows
		.map((r) => (r.is_dir ? `\x1b[1;34m${r.name}/\x1b[0m` : `${r.name}  ${r.size}B`))
		.join('\r\n');
}

function catOutput(arg: string): string {
	const rel = resolvePath(arg).replace(/^\/preview\/wf-agent\/?/, '');
	const files = allFiles();
	if (rel && files.has(rel)) {
		return (files.get(rel) ?? '').replace(/\n/g, '\r\n');
	}
	return `cat: ${arg || '.'}: no such file in the preview workspace`;
}

/** Resolve one input line into the shell's textual response. */
function runCommand(line: string): { output: string; exit?: number } | null {
	const [cmd, ...rest] = line.trim().split(/\s+/);
	const arg = rest.join(' ');
	switch (cmd) {
		case '':
			return null;
		case 'help':
			return { output: HELP };
		case 'ls':
			return { output: listOutput(arg) };
		case 'cat':
			return { output: catOutput(arg) };
		case 'pwd':
			return { output: CWD };
		case 'echo':
			return { output: arg };
		case 'whoami':
			return { output: 'preview-user' };
		case 'clear':
			return { output: '\x1b[2J\x1b[H' };
		case 'exit':
			return { output: 'logout', exit: 0 };
		default:
			return {
				output: `sh: command not found: ${cmd} (preview shell supports: help, ls, cat, pwd, echo, whoami, clear, exit)`,
			};
	}
}

/** Total output bytes streamed per session; the next connect reports it as the replay cursor. */
const sessionCursors = new Map<string, number>();

const byteCounter = new TextEncoder();

function byteLength(text: string): number {
	for (let i = 0; i < text.length; i += 1) {
		if (text.charCodeAt(i) > 127) return byteCounter.encode(text).length;
	}
	return text.length;
}

/** Binary control frame matching the server wire format: 0x00 followed by JSON. */
function controlFrame(payload: unknown): ArrayBuffer {
	const json = byteCounter.encode(JSON.stringify(payload));
	const frame = new Uint8Array(1 + json.length);
	frame[0] = 0x00;
	frame.set(json, 1);
	return frame.buffer;
}

function resumeCursorFromUrl(url: string): number {
	try {
		const parsed = new URL(url, 'http://localhost');
		const raw = parsed.searchParams.get('cursor');
		const next = raw === null ? 0 : Number(raw);
		return Number.isSafeInteger(next) && next >= 0 ? next : 0;
	} catch {
		return 0;
	}
}

class MockPtySocket {
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSING = 2;
	static readonly CLOSED = 3;

	readyState = MockPtySocket.CONNECTING;
	binaryType = 'arraybuffer';
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	onopen: ((event?: any) => void) | null = null;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	onmessage: ((event: any) => void) | null = null;
	onclose: ((event: CloseEvent) => void) | null = null;
	onerror: (() => void) | null = null;

	#inputBuffer = '';
	#closed = false;
	#inEscape = false;
	#skipLineFeed = false;
	#outputQueue: Array<() => void> = [];
	#draining = false;
	#sessionId: string | null = null;
	#resumeCursor = 0;

	constructor(url: string | URL) {
		const target = String(url);
		this.#sessionId = sessionIdFromUrl(target);
		this.#resumeCursor = resumeCursorFromUrl(target);
		setTimeout(() => {
			if (this.#closed) return;
			this.readyState = MockPtySocket.OPEN;
			this.onopen?.({ type: 'open' });
			// Mirror the real attach contract: unknown ids close with 4004 so
			// the terminal view marks the stale tab exited without touching
			// its title; exited sessions replay the exited frame then 4010.
			const session = this.#sessionId ? getSession(this.#sessionId) : undefined;
			if (!session) {
				this.close(4004, 'pty session not found (preview)');
				return;
			}
			if (session.status !== 'running') {
				const known = this.#sessionId ? (sessionCursors.get(this.#sessionId) ?? 0) : 0;
				const cursor = Math.max(known, this.#resumeCursor);
				this.#deliverControl({ cursor });
				this.#enqueue(() => {
					this.#deliverControl({
						event: 'exited',
						exit_code: session.exit_code ?? 0,
						reason: 'preview shell exit',
					});
				});
				this.#enqueue(() => {
					this.close(4010, 'pty exited (preview shell exit)');
				});
				return;
			}
			// Replay contract first: the absolute cursor lets the client resume later.
			const known = this.#sessionId ? (sessionCursors.get(this.#sessionId) ?? 0) : 0;
			const cursor = Math.max(known, this.#resumeCursor);
			this.#deliverControl({ cursor });
			// A reconnecting client already holds the earlier output in its
			// restored buffer, so only a fresh session gets the greeting.
			if (cursor === 0) {
				this.#stream(WELCOME + '\r\n');
				this.#stream(PROMPT);
			}
		}, 0);
	}

	send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
		if (this.readyState !== MockPtySocket.OPEN || typeof data !== 'string') return;
		for (const char of data) this.#handleChar(char);
	}

	close(code = 1000, reason = ''): void {
		if (this.readyState === MockPtySocket.CLOSED) return;
		this.readyState = MockPtySocket.CLOSED;
		this.#closed = true;
		this.#outputQueue.length = 0;
		this.onclose?.({ code, reason } as CloseEvent);
	}

	/** Cooked-line editing with immediate echo, like a real PTY. */
	#handleChar(char: string): void {
		if (this.#inEscape) {
			// Escape sequences (arrow keys, etc.) carry no line content here.
			if (/[a-zA-Z~]/.test(char)) this.#inEscape = false;
			return;
		}
		if (char === '\x1b') {
			this.#inEscape = true;
			return;
		}
		if (char === '\r' || char === '\n') {
			// Collapse a CR LF pair into a single submit.
			if (char === '\n' && this.#skipLineFeed) {
				this.#skipLineFeed = false;
				return;
			}
			if (char === '\r') this.#skipLineFeed = true;
			this.#emit('\r\n');
			const line = this.#inputBuffer;
			this.#inputBuffer = '';
			this.#handleLine(line);
			return;
		}
		this.#skipLineFeed = false;
		if (char === '\x03') {
			// INTR abandons the cooked line and redraws the prompt, like a real PTY.
			this.#inputBuffer = '';
			this.#emit('^C\r\n');
			this.#stream(PROMPT);
			return;
		}
		if (char === '\x7f' || char === '\b') {
			if (this.#inputBuffer.length === 0) return;
			const kept = [...this.#inputBuffer];
			kept.pop();
			this.#inputBuffer = kept.join('');
			this.#emit('\b \b');
			return;
		}
		if (char < ' ' && char !== '\t') return;
		this.#inputBuffer += char;
		this.#emit(char);
	}

	#handleLine(line: string): void {
		// Input was already echoed per keystroke; just answer the command.
		const result = runCommand(line);
		if (!result) {
			this.#stream(PROMPT);
			return;
		}
		this.#stream(result.output + '\r\n');
		if (result.exit !== undefined) {
			this.#stream(`[session ended with code ${result.exit}]\r\n`);
			// Control frame and close travel through the same queue so they
			// land after the delayed output above, matching the server order
			// of data frames then the exited frame.
			const exitCode = result.exit;
			this.#enqueue(() => {
				this.#deliverControl({
					event: 'exited',
					exit_code: exitCode,
					reason: 'preview shell exit',
				});
			});
			this.#enqueue(() => {
				if (this.#sessionId) exitSession(this.#sessionId, exitCode);
				this.close(4010, 'pty exited (preview shell exit)');
			});
			return;
		}
		this.#stream(PROMPT);
	}

	#countDelivered(text: string): void {
		if (!this.#sessionId) return;
		sessionCursors.set(
			this.#sessionId,
			(sessionCursors.get(this.#sessionId) ?? 0) + byteLength(text),
		);
	}

	/** Immediate echo path for typed input; no chunk delay so typing feels live. */
	#emit(text: string): void {
		if (this.#closed || this.readyState !== MockPtySocket.OPEN) return;
		this.#countDelivered(text);
		this.onmessage?.({ data: text });
	}

	#deliverControl(payload: unknown): void {
		if (this.#closed || this.readyState !== MockPtySocket.OPEN) return;
		this.onmessage?.({ data: controlFrame(payload) });
	}

	/** Queue one ordered output step behind everything already scheduled. */
	#enqueue(step: () => void): void {
		if (this.#closed) return;
		this.#outputQueue.push(step);
		this.#drain();
	}

	/** Push `text` in small delayed chunks so the terminal renders
	 *  incrementally, the way a real PTY delivers bytes. Every chunk joins
	 *  a single queue, so concurrent responses can never overtake each other. */
	#stream(text: string): void {
		if (this.#closed) return;
		for (const chunk of chunkText(text)) {
			this.#outputQueue.push(() => {
				if (this.#closed || this.readyState !== MockPtySocket.OPEN) return;
				this.#countDelivered(chunk);
				this.onmessage?.({ data: chunk });
			});
		}
		this.#drain();
	}

	#drain(): void {
		if (this.#draining) return;
		this.#draining = true;
		const step = (): void => {
			if (this.#closed) {
				this.#outputQueue.length = 0;
				this.#draining = false;
				return;
			}
			const next = this.#outputQueue.shift();
			if (!next) {
				this.#draining = false;
				return;
			}
			next();
			setTimeout(step, OUTPUT_CHUNK_MS);
		};
		setTimeout(step, OUTPUT_CHUNK_MS);
	}
}

function sessionIdFromUrl(url: string): string | null {
	const match = /\/api\/v1\/pty\/([^/]+)\/connect/.exec(url);
	return match?.[1] ?? null;
}

/** Install the selective WebSocket override (preview mode only). */
export function installMockPtySocket(): void {
	if (typeof window === 'undefined') return;
	const NativeWebSocket = window.WebSocket;
	const Patched = function (this: unknown, url: string | URL, protocols?: string | string[]) {
		const target = String(url);
		if (target.includes(MOCK_URL_MARKER)) {
			return new MockPtySocket(target) as unknown as WebSocket;
		}
		return protocols
			? new (NativeWebSocket as new (u: string, p: string | string[]) => WebSocket)(
					target,
					protocols,
				)
			: new (NativeWebSocket as new (u: string) => WebSocket)(target);
	} as unknown as typeof WebSocket;
	Object.assign(Patched, {
		prototype: NativeWebSocket.prototype,
		CONNECTING: WebSocket.CONNECTING,
		OPEN: WebSocket.OPEN,
		CLOSING: WebSocket.CLOSING,
		CLOSED: WebSocket.CLOSED,
	});
	window.WebSocket = Patched;
}

// Keep the unused import of getSession referenced via a tiny helper used by
// future reconnect flows; tree-shaken when unreferenced.
export function previewSessionAlive(id: string): boolean {
	return getSession(id)?.status === 'running';
}
