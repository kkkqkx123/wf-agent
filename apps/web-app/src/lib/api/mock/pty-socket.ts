/**
 * Fixture-backed WebSocket for the PTY terminal in preview mode.
 *
 * `connectPty` opens a real WebSocket against `${API_BASE_URL}/api/v1/pty/{id}/connect`,
 * which no preview backend serves. Install a selective override: URLs pointing
 * at the mock base get an in-memory socket that goes OPEN immediately and
 * echoes input back, so the preview terminal stays interactive; every other
 * URL keeps the native WebSocket so real transports are untouched.
 */

const MOCK_URL_MARKER = '/api/v1/pty/';

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

	constructor(_url: string | URL) {
		void _url;
		setTimeout(() => {
			this.readyState = MockPtySocket.OPEN;
			this.onopen?.({ type: 'open' });
		}, 0);
	}

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
		if (this.readyState !== MockPtySocket.OPEN || typeof data !== 'string') return;
		setTimeout(() => {
			this.onmessage?.({ data: data });
		}, 0);
	}

	close(): void {
		if (this.readyState === MockPtySocket.CLOSED) return;
		this.readyState = MockPtySocket.CLOSED;
		this.onclose?.();
	}
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
