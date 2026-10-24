/**
 * Fixture-backed replacement for openapi-fetch's client.
 *
 * web-app-preview never talks to a real wf-server. Every call to
 * `client.GET` / `client.POST` below resolves instantly against the
 * hard-coded sample data in `#lib/fixtures`. The return shape matches
 * openapi-fetch ({ data, error, response }) so `envelope.ts` and the
 * services layer stay completely unchanged.
 *
 * This file is a "preview-only override": sync-frontend-preview.sh copies
 * it *once* from the source tree, then excludes it from future syncs so
 * edits here never get overwritten.
 *
 * The mock internals live under `./mock/`: the router kernel
 * (`mock/router.ts`), per-domain route registrations (`mock/routes/*`,
 * wired in registration order by `mock/registry.ts`), wire adapters
 * (`mock/wire.ts`) and the fixture SSE stream (`mock/event-stream.ts`).
 * This facade keeps the public surface (`client`, `request`,
 * `downloadFile`, `API_BASE_URL`, `resolveApiKey`) that mirrored files
 * import, so shared code never sees the split.
 */

import { makeRouter, missingRoute, okEnvelope, queryOf } from './mock/router';
import { registerAllRoutes } from './mock/registry';
import './mock/event-stream';

// ---------------------------------------------------------------------------
// openapi-fetch type — preview never runs typecheck against the schema
// (we only need the runtime shape), so importing paths is intentionally
// skipped. Using `any` keeps preview independent from schema.d.ts rebuilds.
// ---------------------------------------------------------------------------
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyClient = any;

// Re-export an empty baseUrl so shared code that reads it still compiles.
export const API_BASE_URL = '/mock';

// ---------------------------------------------------------------------------
// Router + route registration
// ---------------------------------------------------------------------------

const router = makeRouter();
registerAllRoutes(router);

// ---------------------------------------------------------------------------
// Public client object — mirrors openapi-fetch surface used by services
// ---------------------------------------------------------------------------

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

export const client: AnyClient = {
	// openapi-fetch middleware hooks — no-ops in preview
	use() {},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async GET(path: string, opts?: any): Promise<any> {
		await delay(30); // simulate a tiny network latency
		const query = queryOf(opts?.params);
		const data = router.dispatch('GET', path, query, undefined);
		if (data === undefined) return missingRoute('GET', path);
		return okEnvelope(data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async POST(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const body = opts?.body;
		const data = router.dispatch('POST', path, query, body);
		if (data === undefined) return missingRoute('POST', path);
		return okEnvelope(data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async PUT(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const body = opts?.body;
		const data = router.dispatch('PUT', path, query, body);
		if (data === undefined) return missingRoute('PUT', path);
		return okEnvelope(data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async DELETE(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const data = router.dispatch('DELETE', path, query, undefined);
		if (data === undefined) return missingRoute('DELETE', path);
		return okEnvelope(data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async PATCH(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const body = opts?.body;
		const data = router.dispatch('PATCH', path, query, body);
		if (data === undefined) return missingRoute('PATCH', path);
		return okEnvelope(data);
	},
};

// ---------------------------------------------------------------------------
// request / downloadFile — same call shape as the formal client
// ---------------------------------------------------------------------------

export async function request(
	method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
	path: string,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	opts?: any,
): Promise<{ data?: unknown; error?: unknown }> {
	await delay(30);
	const query = queryOf(opts?.params);
	const body = opts?.body;
	const data = router.dispatch(method, path, query, body);
	if (data === undefined) return missingRoute(method, path);
	return okEnvelope(data);
}

export async function downloadFile(
	path: string,
	fallbackFilename: string,
	init?: { method?: 'GET' | 'POST'; body?: unknown },
): Promise<void> {
	const method = init?.method ?? 'GET';
	const [urlPath, queryString] = path.split('?');
	const query: Record<string, unknown> = {};
	if (queryString) {
		for (const [key, value] of new URLSearchParams(queryString)) {
			query[key] = value;
		}
	}
	const data = router.dispatch(method, urlPath, query, init?.body);
	const blob = new Blob([JSON.stringify(data ?? null, null, 2)], {
		type: 'application/json',
	});
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = fallbackFilename;
	anchor.click();
	URL.revokeObjectURL(url);
}

// ---------------------------------------------------------------------------
// resolveApiKey — no-op in preview (no backend to authenticate against)
// ---------------------------------------------------------------------------
export function resolveApiKey(): string | undefined {
	return undefined;
}
