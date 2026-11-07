/** Mock routes: PTY sessions and single-use stream tickets. */

import type { MockRouter } from '../router';
import { mockError } from './fs';
import { notifySessionRemoved } from '../pty-socket';
import {
	createSession,
	getSession,
	listSessions,
	removeSession,
	renameSession,
	resizeSession,
} from '../pty-sessions';

export function registerPty(router: MockRouter): void {
	// ---------- pty ----------
	router.on('POST', '/api/v1/pty', (_p, _q, body) => {
		const b = (body ?? {}) as {
			rows?: number;
			cols?: number;
			title?: string;
			title_source?: 'default' | 'auto' | 'manual';
		};
		return createSession(b);
	});
	router.on('GET', '/api/v1/pty', () => listSessions());
	router.on('GET', '/api/v1/pty/{id}', (params) => {
		const session = getSession(params.id);
		// Mirror the real backend: unknown ids are 404 so the session store
		// drops them instead of collapsing every stale tab to one fake title.
		if (!session) return mockError(404, 'NOT_FOUND', `PTY session not found: ${params.id}`);
		return session;
	});
	router.on('PUT', '/api/v1/pty/{id}', (params, _q, body) => {
		if (!getSession(params.id)) {
			return mockError(404, 'NOT_FOUND', `PTY session not found: ${params.id}`);
		}
		const b = (body ?? {}) as {
			rows?: number;
			cols?: number;
			title?: string;
			title_source?: 'default' | 'auto' | 'manual';
		};
		resizeSession(params.id, b.rows, b.cols);
		renameSession(params.id, b.title, b.title_source);
		return getSession(params.id);
	});
	router.on('DELETE', '/api/v1/pty/{id}', (params) => {
		const removed = removeSession(params.id);
		notifySessionRemoved(params.id);
		if (!removed) return mockError(404, 'NOT_FOUND', `PTY session not found: ${params.id}`);
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
