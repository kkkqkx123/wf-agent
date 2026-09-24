import createClient from 'openapi-fetch';
import type { paths } from './schema';

/**
 * Base URL for the wf-server API.
 * Priority: VITE_API_BASE_URL env var → same origin + /api/v1.
 * In dev mode the Vite proxy forwards /api/* to the backend.
 */
const _origin = typeof location !== 'undefined' ? location.origin : 'http://localhost';
export const API_BASE_URL =
	import.meta.env.VITE_API_BASE_URL ?? `${_origin}/api/v1`;

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

/** Interceptor: inject x-api-key on every request. */
client.use({
	async onRequest({ request }) {
		const key = resolveApiKey();
		if (key) request.headers.set('x-api-key', key);
		return request;
	},
});
