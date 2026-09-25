import createClient from 'openapi-fetch';
import type { paths } from './schema';

/**
 * Base URL every request is built on: VITE_API_BASE_URL → same origin.
 * openapi-fetch concatenates it with the operation path, and those paths
 * already carry the `/api/v1` segment, so the base must stop at the origin.
 * In dev mode the Vite proxy forwards /api/* to the backend.
 */
const _origin =
	typeof location !== 'undefined' ? location.origin : 'http://localhost';
export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? _origin;

/**
 * Resolve API key from environment or localStorage.
 * The key is optional — when AUTH_ENABLED=false on the backend,
 * no key is required.
 */
export function resolveApiKey(): string | undefined {
	const fromEnv = import.meta.env.VITE_API_KEY as string | undefined;
	if (fromEnv && fromEnv.length > 0) return fromEnv;
	if (typeof localStorage !== 'undefined') {
		const fromStorage = localStorage.getItem('wf.apiKey');
		if (fromStorage && fromStorage.length > 0) return fromStorage;
	}
	return undefined;
}

export const client = createClient<paths>({
	baseUrl: API_BASE_URL,
	headers: { 'Content-Type': 'application/json' },
});

/**
 * Escape hatch for routes whose generated operation types disagree with their
 * own URL template: utoipa emits one `handle_*` name for several routes, so the
 * shared `operations[...]` type describes a different route. Keeping the cast
 * here leaves every other call site on the typed client.
 */
export function request(
	method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
	path: string,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	init?: Record<string, any>,
): Promise<{ data?: unknown; error?: unknown }> {
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	return (client as any)[method](path, init ?? {});
}

/**
 * Fetch a full-path endpoint and hand the response to the browser as a file
 * download. Export routes answer with an attachment instead of the JSON
 * envelope, so they bypass the typed client.
 */
export async function downloadFile(
	path: string,
	fallbackFilename: string,
	init?: { method?: 'GET' | 'POST'; body?: unknown },
): Promise<void> {
	const key = resolveApiKey();
	const response = await fetch(`${API_BASE_URL}${path}`, {
		method: init?.method ?? 'GET',
		headers: {
			...(init?.body === undefined
				? {}
				: { 'Content-Type': 'application/json' }),
			...(key ? { 'x-api-key': key } : {}),
		},
		body: init?.body === undefined ? undefined : JSON.stringify(init.body),
	});
	if (!response.ok) {
		throw new Error(`Download failed (HTTP ${response.status})`);
	}
	const disposition = response.headers.get('content-disposition') ?? '';
	const named = /filename\*?=(?:UTF-8'')?"?([^";]+)"?/i.exec(disposition);
	const url = URL.createObjectURL(await response.blob());
	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = named?.[1]?.trim() ?? fallbackFilename;
	anchor.click();
	URL.revokeObjectURL(url);
}

/** Interceptor: inject x-api-key on every request. */
client.use({
	async onRequest({ request }) {
		const key = resolveApiKey();
		if (key) request.headers.set('x-api-key', key);
		return request;
	},
});
