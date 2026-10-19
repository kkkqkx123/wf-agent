/** Mock routes: trigger history, trigger executions and hooks. */

import { triggerRecords } from '$lib/fixtures/triggers';
import { camelToSnakeDeep, pageView, type MockRouter } from '../router';

export function registerTriggers(router: MockRouter): void {
	// ---------- triggers ----------
	router.on('GET', '/api/v1/triggers/history', (_p, query) => {
		return pageView(camelToSnakeDeep(triggerRecords), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/trigger-executions', (_p, query) => {
		return pageView(camelToSnakeDeep(triggerRecords), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('POST', '/api/v1/trigger-executions/cleanup', () => {
		// The backend answers with a bare removed-count number; the
		// fixture sandbox reports the fixture size as removed.
		return triggerRecords.length;
	});
	router.on('POST', '/api/v1/hooks/{name}', () => ({
		status: 'fired',
		detail: 'Preview dispatch accepted; no live hook endpoint behind the mock.',
	}));
}
