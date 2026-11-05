/** Mock routes: PTY sessions and single-use stream tickets. */

import type { MockRouter } from '../router';
import {
	createSession,
	exitSession,
	getSession,
	listSessions,
	removeSession,
	renameSession,
	resizeSession,
} from '../pty-sessions';

export function registerPty(router: MockRouter): void {
	// ---------- pty ----------
	router.on('POST', '/api/v1/pty', (_p, _q, body) => {
		const b = (body ?? {}) as { rows?: number; cols?: number; title?: string };
		return createSession(b);
	});
	router.on('GET', '/api/v1/pty', () => listSessions());
	router.on('GET', '/api/v1/pty/{id}', (params) => {
		const session = getSession(params.id);
		return session ?? { id: params.id, cwd: '/preview/wf-agent', title: 'Terminal 1', status: 'exited', exit_code: 127, pid: 0, rows: 24, cols: 80 };
	});
	router.on('PUT', '/api/v1/pty/{id}', (params, _q, body) => {
		const b = (body ?? {}) as { rows?: number; cols?: number; title?: string };
		const session = resizeSession(params.id, b.rows, b.cols);
		return renameSession(params.id, b.title) ?? session ?? { id: params.id, cwd: '/preview/wf-agent', title: 'Terminal 1', status: 'exited', exit_code: 127, pid: 0, rows: 24, cols: 80 };
	});
	router.on('DELETE', '/api/v1/pty/{id}', (params) => {
		const removed = removeSession(params.id);
		if (!removed) exitSession(params.id, 0);
		return { removed: true };
	});

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
