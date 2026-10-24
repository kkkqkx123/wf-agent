/** Mock routes: PTY sessions and single-use stream tickets. */

import type { MockRouter } from '../router';

let previewPtySeq = 0;

export function registerPty(router: MockRouter): void {
	// ---------- pty ----------
	router.on('POST', '/api/v1/pty', (_p, _q, body) => {
		const b = (body ?? {}) as { rows?: number; cols?: number };
		previewPtySeq += 1;
		return {
			id: `pty-preview-${previewPtySeq}`,
			cwd: '/preview/wf-agent',
			status: 'running',
			exit_code: null,
			pid: 42000 + previewPtySeq,
			rows: b.rows ?? 24,
			cols: b.cols ?? 80,
		};
	});
	router.on('GET', '/api/v1/pty', () => ([]));
	router.on('GET', '/api/v1/pty/{id}', (params) => ({
		id: params.id,
		cwd: '/preview/wf-agent',
		status: 'running',
		exit_code: null,
		pid: 42000,
		rows: 24,
		cols: 80,
	}));
	router.on('POST', '/api/v1/pty/{id}/resize', (params, _q, body) => {
		const b = (body ?? {}) as { rows?: number; cols?: number };
		return {
			id: params.id,
			cwd: '/preview/wf-agent',
			status: 'running',
			exit_code: null,
			pid: 42000,
			rows: b.rows ?? 24,
			cols: b.cols ?? 80,
		};
	});
	router.on('DELETE', '/api/v1/pty/{id}', () => ({ removed: true }));

	// Single-use stream tickets: the shared frontend mints a ticket before the
	// websocket / SSE handshake so the persistent key never enters the stream URL.
	router.on('POST', '/api/v1/pty/{id}/connect-token', () => ({
		ticket: 'preview-pty-ticket',
		expires_in: 60,
	}));
	router.on('POST', '/api/v1/events/stream-ticket', () => ({
		ticket: 'preview-stream-ticket',
		expires_in: 60,
	}));
}
