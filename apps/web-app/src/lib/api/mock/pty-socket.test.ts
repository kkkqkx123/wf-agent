import { describe, expect, it } from 'vitest';
import { installMockPtySocket, previewSessionAlive } from './pty-socket';
import { createSession } from './pty-sessions';

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function mockSocket(url: string): {
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	socket: any;
	texts: string[];
	binaries: ArrayBuffer[];
	closes: Array<{ code: number; reason: string }>;
	opened: () => boolean;
} {
	const g = globalThis as unknown as {
		window?: { WebSocket?: unknown };
	};
	if (!g.window) {
		class NativeStub {}
		g.window = { WebSocket: NativeStub };
	}
	installMockPtySocket();
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	const WS = (g.window as any).WebSocket as new (url: string) => any;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	const socket: any = new WS(url);
	const texts: string[] = [];
	const binaries: ArrayBuffer[] = [];
	const closes: Array<{ code: number; reason: string }> = [];
	let opened = false;
	socket.onopen = () => {
		opened = true;
	};
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	socket.onmessage = (event: any) => {
		if (event.data instanceof ArrayBuffer) binaries.push(event.data);
		else texts.push(event.data);
	};
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	socket.onclose = (event: any) => {
		closes.push({ code: event.code, reason: event.reason });
	};
	return { socket, texts, binaries, closes, opened: () => opened };
}

function decodeControl(frame: ArrayBuffer): Record<string, unknown> {
	const bytes = new Uint8Array(frame);
	expect(bytes[0]).toBe(0x00);
	return JSON.parse(new TextDecoder().decode(bytes.subarray(1))) as Record<string, unknown>;
}

describe('preview mock PTY socket', () => {
	it('delivers the replay cursor control frame before any text', async () => {
		const session = createSession({});
		const { socket, texts, binaries, opened } = mockSocket(
			`ws://localhost/mock/api/v1/pty/${session.id}/connect?cursor=0&ticket=t`,
		);
		await sleep(80);
		expect(opened()).toBe(true);
		expect(binaries.length).toBeGreaterThanOrEqual(1);
		expect(decodeControl(binaries[0]).cursor).toBe(0);
		// The greeting must arrive whole: no prompt chunk may slip into the middle.
		expect(texts.join('')).toContain("Type 'help' to list the supported commands.");
		socket.close();
	});

	it('echoes keystrokes immediately and edits the line on backspace', async () => {
		const session = createSession({});
		const { socket, texts } = mockSocket(
			`ws://localhost/mock/api/v1/pty/${session.id}/connect?cursor=0&ticket=t`,
		);
		await sleep(30);
		texts.length = 0;
		socket.send('h');
		socket.send('i');
		expect(texts).toEqual(['h', 'i']);
		socket.send('\x7f');
		expect(texts).toEqual(['h', 'i', '\b \b']);
		socket.send('\x7f');
		socket.send('help\r');
		await sleep(400);
		const joined = texts.join('');
		expect(joined).toContain('Supported preview commands');
		// The next prompt lands after the command output, never inside it.
		expect(joined.lastIndexOf('~/wf-agent')).toBeGreaterThan(
			joined.indexOf('Supported preview commands'),
		);
		socket.close();
	});

	it('ends the session with an exited control frame and close code 4010', async () => {
		const session = createSession({});
		const { socket, binaries, closes } = mockSocket(
			`ws://localhost/mock/api/v1/pty/${session.id}/connect?cursor=0&ticket=t`,
		);
		await sleep(30);
		binaries.length = 0;
		socket.send('exit\r');
		await sleep(250);
		const exited = binaries.map(decodeControl).find((meta) => meta.event === 'exited');
		expect(exited?.exit_code).toBe(0);
		expect(closes.length).toBe(1);
		expect(closes[0].code).toBe(4010);
		expect(previewSessionAlive(session.id)).toBe(false);
	});

	it('closes unknown sessions with 4004 like the real attach path', async () => {
		const { socket, texts, binaries, closes, opened } = mockSocket(
			'ws://localhost/mock/api/v1/pty/pty-unknown-stale-id/connect?cursor=0&ticket=t',
		);
		await sleep(80);
		expect(opened()).toBe(true);
		// No shell output for a session the server never knew.
		expect(texts.join('')).toBe('');
		expect(binaries.length).toBe(0);
		expect(closes.length).toBe(1);
		expect(closes[0].code).toBe(4004);
		socket.close();
	});

	it('abandons the cooked line on INTR and redraws the prompt', async () => {
		const session = createSession({});
		const { socket, texts } = mockSocket(
			`ws://localhost/mock/api/v1/pty/${session.id}/connect?cursor=0&ticket=t`,
		);
		await sleep(30);
		texts.length = 0;
		socket.send('ls');
		socket.send('\x03');
		socket.send('pwd\r');
		await sleep(400);
		const joined = texts.join('');
		// The abandoned 'ls' never runs; a fresh prompt answers 'pwd' instead.
		expect(joined).toContain('^C');
		expect(joined).not.toContain('README.md');
		expect(joined).toContain('/preview/wf-agent');
		socket.close();
	});
});

describe('preview mock event feed socket', () => {
	it('opens /api/v1/ws and acks subscribe without closing', async () => {
		const { socket, texts, closes, opened } = mockSocket('ws://localhost/mock/api/v1/ws');
		await sleep(30);
		expect(opened()).toBe(true);
		socket.send(JSON.stringify({ type: 'subscribe', global: true }));
		await sleep(20);
		expect(texts).toEqual([JSON.stringify({ type: 'subscribed' })]);
		expect(closes.length).toBe(0);
		socket.close();
	});

	it('answers ping with pong so the keepalive never trips a reconnect', async () => {
		const { socket, texts } = mockSocket('ws://localhost/mock/api/v1/ws');
		await sleep(30);
		texts.length = 0;
		socket.send(JSON.stringify({ type: 'ping' }));
		await sleep(20);
		expect(texts).toEqual([JSON.stringify({ type: 'pong' })]);
		socket.close();
	});
});
