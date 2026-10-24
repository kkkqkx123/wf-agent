/** Mock routes: events, health, storage, favorites and analysis. */

import {
	overviewMetrics,
	queryResult,
	perfNodes,
	events,
	dependencies,
	diagnostics,
} from '#lib/fixtures/insights';
import { FIXTURE_EPOCH } from '#lib/fixtures/clock';
import { camelToSnakeDeep, pageView, type MockRouter } from '../router';

export function registerInsights(router: MockRouter): void {
	// ---------- events ----------
	router.on('GET', '/api/v1/events', (_p, query) => {
		return pageView(camelToSnakeDeep(events), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/events/search', (_p, query) => {
		return pageView(camelToSnakeDeep(events), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/events/size', () => {
		return events.length;
	});

	// ---------- dependencies + health ----------
	router.on('GET', '/api/v1/dependencies/audit', () => camelToSnakeDeep(dependencies));
	router.on('GET', '/health', () => ({
		ready: true,
		storage: 'sqlite',
		persistence: { mode: 'single', db: 'wf.db' },
	}));
	router.on('GET', '/api/v1/info', () => ({
		apiVersion: 'v1',
		version: '1.0.0',
		name: 'wf-agent',
	}));
	router.on('GET', '/api/v1/favorites', (_p, query) => pageView([], +(query.limit ?? 50), +(query.offset ?? 0)));
	router.on('PUT', '/api/v1/favorites/{kind}/{id}', () => ({ ok: true }));
	router.on('DELETE', '/api/v1/favorites/{kind}/{id}', () => ({ ok: true }));
	router.on('GET', '/api/v1/storage/diagnose', () => camelToSnakeDeep(diagnostics));
	router.on('GET', '/api/v1/storage/stats', () => ({
		files: 128,
		total_bytes: 4_120_000,
		oldest: new Date(FIXTURE_EPOCH - 86_400_000).toISOString(),
		newest: new Date(FIXTURE_EPOCH).toISOString(),
	}));

	// ---------- insights / analysis ----------
	router.on('POST', '/api/v1/query', () => camelToSnakeDeep(queryResult));
	router.on('GET', '/api/v1/analysis/stats/top-node-types', () => camelToSnakeDeep(perfNodes));
	router.on('GET', '/api/v1/analysis/stats', () => ({
		executions_24h: overviewMetrics.find((m) => m.label === 'Completed 24h')?.value ?? 0,
		running: overviewMetrics.find((m) => m.label === 'Running')?.value ?? 0,
		failed_24h: overviewMetrics.find((m) => m.label === 'Failed 24h')?.value ?? 0,
	}));
}
