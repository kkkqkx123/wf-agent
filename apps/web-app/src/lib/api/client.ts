/**
 * Shared openapi-fetch client scoped to our generated `paths` type.
 *
 * The real client is intentionally tiny — the opinionated bits (envelope
 * peeling, error branching, fixture fallback) live in sibling helpers and
 * `+page.ts` load functions.  Keeping this thin makes SvelteKit SSR work
 * out of the box: callers just spread `{ fetch }` from the load context.
 */
import createClient from 'openapi-fetch';
import type { paths } from './schema';

function resolveBaseUrl(): string {
	// Dev  → Vite proxy rewrites `/api` → backend
	// Prod → `--static-dir` hosts backend + static on the same origin
	return import.meta.env.VITE_API_BASE ?? '/api/v1';
}

function resolveApiKey(): string | undefined {
	if (import.meta.env.VITE_API_KEY) return import.meta.env.VITE_API_KEY;
	// Backend defaults AUTH_ENABLED=false in dev; no key required.
	return undefined;
}

export const client = createClient<paths>({
	baseUrl: resolveBaseUrl(),
	headers: {
		...(resolveApiKey() ? { 'x-api-key': resolveApiKey()! } : {}),
		accept: 'application/json',
	},
});
