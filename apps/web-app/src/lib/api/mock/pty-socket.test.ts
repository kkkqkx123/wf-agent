import { describe, expect, it } from 'vitest';
import { installMockPtySocket, previewSessionAlive } from './pty-socket';

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
		const { socket, texts, binaries, opened } = mockSocket(
			'ws://localhost/mock/api/v1/pty/pty-handshake-1/connect?cursor=0&ticket=t',
		);
		await sleep(80);
		expect(opened()).toBe(true);
		expect(binaries.length).toBeGreaterThanOrEqual(1);
		expect(decodeControl(binaries[0]).cursor).toBe(0);
		expect(texts.join('')).toContain('preview shell');
		socket.close();
	});

	it('echoes keystrokes immediately and edits the line on backspace', async () => {
		const { socket, texts } = mockSocket(
			'ws://localhost/mock/api/v1/pty/pty-echo-1/connect?cursor=0&ticket=t',
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
		await sleep(250);
		expect(texts.join('')).toContain('Supported preview commands');
		socket.close();
	});

	it('ends the session with an exited control frame and close code 4010', async () => {
		const { socket, binaries, closes } = mockSocket(
			'ws://localhost/mock/api/v1/pty/pty-exit-1/connect?cursor=0&ticket=t',
		);
		await sleep(30);
		binaries.length = 0;
		socket.send('exit\r');
		await sleep(250);
		const exited = binaries.map(decodeControl).find((meta) => meta.event === 'exited');
		expect(exited?.exit_code).toBe(0);
		expect(closes.length).toBe(1);
		expect(closes[0].code).toBe(4010);
		expect(previewSessionAlive('pty-exit-1')).toBe(false);
	});
});
