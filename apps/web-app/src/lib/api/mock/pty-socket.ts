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

function chunkText(text: string): string[] {
	const chunks: string[] = [];
	for (let i = 0; i < text.length; i += 64) {
		chunks.push(text.slice(i, i + 64));
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
			return { output: `sh: command not found: ${cmd} (preview shell supports: help, ls, cat, pwd, echo, whoami, clear, exit)` };
	}
}

class MockPtySocket {
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSING = 2;
	static readonly CLOSED = 3;

	readyState = MockPtySocket.CONNECTING;
	binaryType = 'blob';
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	onopen: ((event?: any) => void) | null = null;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	onmessage: ((event: any) => void) | null = null;
	onclose: (() => void) | null = null;
	onerror: (() => void) | null = null;

	#inputBuffer = '';
	#closed = false;
	#sessionId: string | null = null;

	constructor(url: string | URL) {
		this.#sessionId = sessionIdFromUrl(String(url));
		setTimeout(() => {
			this.readyState = MockPtySocket.OPEN;
			this.onopen?.({ type: 'open' });
			this.#stream(WELCOME + '\r\n');
			this.#stream(PROMPT);
		}, 0);
	}

	send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
		if (this.readyState !== MockPtySocket.OPEN || typeof data !== 'string') return;
		this.#inputBuffer += data;
		let newline = this.#inputBuffer.indexOf('\r');
		while (newline >= 0) {
			const line = this.#inputBuffer.slice(0, newline);
			this.#inputBuffer = this.#inputBuffer.slice(newline + 1);
			this.#handleLine(line);
			newline = this.#inputBuffer.indexOf('\r');
		}
	}

	close(): void {
		if (this.readyState === MockPtySocket.CLOSED) return;
		this.readyState = MockPtySocket.CLOSED;
		this.#closed = true;
		this.onclose?.();
	}

	#handleLine(line: string): void {
		// Echo the typed line so the terminal behaves like a cooked PTY.
		this.#stream(`${line}\r\n`);
		const result = runCommand(line);
		if (!result) {
			this.#stream(PROMPT);
			return;
		}
		this.#stream(result.output + '\r\n');
		if (result.exit !== undefined) {
			this.#stream(`[session ended with code ${result.exit}]\r\n`);
			setTimeout(() => {
				if (this.#sessionId) exitSession(this.#sessionId, result.exit);
				this.close();
			}, 30);
			return;
		}
		this.#stream(PROMPT);
	}

	/** Push `text` in small delayed chunks so the terminal renders
	 *  incrementally, the way a real PTY delivers bytes. */
	#stream(text: string): void {
		if (this.#closed) return;
		const chunks = chunkText(text);
		let delay = 0;
		for (const chunk of chunks) {
			delay += 15;
			setTimeout(() => {
				if (this.#closed || this.readyState !== MockPtySocket.OPEN) return;
				this.onmessage?.({ data: chunk });
			}, delay);
		}
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
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	const Patched = function (this: unknown, url: string | URL, protocols?: string | string[]) {
		const target = String(url);
		if (target.includes(MOCK_URL_MARKER)) {
			return new MockPtySocket(target) as unknown as WebSocket;
		}
		return protocols
			? new (NativeWebSocket as new (u: string, p: string | string[]) => WebSocket)(target, protocols)
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
