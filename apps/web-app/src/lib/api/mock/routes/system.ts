/** Mock routes: unified tasks, transport health, server status/config. */

import type { MockRouter } from '../router';

export function registerSystem(router: MockRouter): void {
	router.on('GET', '/api/v1/system/tasks', () => ({
		trigger_executions: 3,
		agent_loops: 2,
		workflows: 1,
	}));
	router.on('GET', '/system/event-health', () => ({
		sse: { live: 1, rejected: 0, overflowed: 0, max: 64 },
		ws: { live: 1 },
	}));
	router.on('GET', '/api/v1/server/status', () => ({
		stopped: false,
		version: 'preview',
		pid: 0,
	}));
	router.on('GET', '/api/v1/server/config', () => ({
		entries: [
			{ key: 'auth.enabled', value: 'false' },
			{ key: 'cors.allowed_origins', value: '*' },
			{ key: 'directory.allow_any_explicit', value: 'true' },
		],
	}));
	router.on('PUT', '/api/v1/server/config', () => ({
		__mockError: true,
		status: 503,
		code: 'SERVICE_UNAVAILABLE',
		message: 'Config updates unavailable',
	}));
	router.on('GET', '/api/v1/system/debug/heap', () => ({
		__mockError: true,
		status: 503,
		code: 'SERVICE_UNAVAILABLE',
		message: 'Heap snapshots are not supported on this platform',
	}));
}
