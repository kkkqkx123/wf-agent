/** Mock routes: node / trigger / workflow / agent templates and imports. */

import { templates } from '#lib/fixtures/insights';
import { camelToSnakeDeep, pageView, type MockRouter } from '../router';

export function registerTemplates(router: MockRouter): void {
	// ---------- templates ----------
	router.on('GET', '/api/v1/templates/library', (_p, query) => {
		void query;
		// The real endpoint answers with a bare array; usage counts use the
		// backend field name so the shared service maps them directly.
		return camelToSnakeDeep(
			templates.map((t) => ({ ...t, usage_count: t.usage })),
		);
	});
	router.on('GET', '/api/v1/templates/library/featured', () => {
		return camelToSnakeDeep(templates.filter((t) => t.featured));
	});
	router.on('GET', '/api/v1/templates/library/popular', () => {
		return camelToSnakeDeep([...templates].sort((a, b) => b.usage - a.usage).slice(0, 6));
	});
	router.on('GET', '/api/v1/templates/node', (_p, query) => {
		const nodeTemplates = templates.filter((t) => t.kind === 'node');
		return pageView(camelToSnakeDeep(nodeTemplates), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/templates/trigger', (_p, query) => {
		const triggerTemplates = templates.filter((t) => t.kind === 'trigger');
		return pageView(camelToSnakeDeep(triggerTemplates), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	// Detail routes: path params never reach the mock, so serve the first
	// fixture of the requested registry as sample data.
	router.on('GET', '/api/v1/templates/node/{id}', () => {
		return camelToSnakeDeep(templates.find((t) => t.kind === 'node') ?? undefined);
	});
	router.on('GET', '/api/v1/templates/trigger/{id}', () => {
		return camelToSnakeDeep(templates.find((t) => t.kind === 'trigger') ?? undefined);
	});
	router.on('GET', '/api/v1/templates/library/workflows/{id}', () => {
		const template = templates.find((t) => t.kind === 'workflow');
		// The library lists published entries only, so the sample entry is
		// published.
		return camelToSnakeDeep({ ...template, is_published: true });
	});
	router.on('GET', '/api/v1/templates/library/agents/{id}', () => {
		return camelToSnakeDeep(templates.find((t) => t.kind === 'agent') ?? undefined);
	});
	// Template imports return a stable preview id; exports return empty payloads.
	router.on('POST', '/api/v1/templates/node/import', () => 'preview-template-id');
	router.on('POST', '/api/v1/templates/trigger/import', () => 'preview-template-id');
	router.on('POST', '/api/v1/templates/library/workflows/import', () => 'preview-template-id');
	router.on('POST', '/api/v1/templates/library/agents/import', () => 'preview-template-id');
	router.on('GET', '/api/v1/templates/library/workflows/{id}/export', () => ({}));
	router.on('GET', '/api/v1/templates/library/agents/{id}/export', () => ({}));
	// Publishing answers with the workflow id it acted on.
	router.on('POST', '/api/v1/templates/library/workflows/{id}/publish', (params) => params.id);
	router.on('POST', '/api/v1/templates/library/workflows/{id}/unpublish', (params) => params.id);
}
