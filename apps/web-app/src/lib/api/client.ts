import createClient from 'openapi-fetch';
import type { paths } from './schema';

/**
 * Base URL every request is built on: VITE_API_BASE_URL → same origin.
 * openapi-fetch concatenates it with the operation path, and those paths
 * already carry the `/api/v1` segment, so the base must stop at the origin.
 * In dev mode the Vite proxy forwards /api/* to the backend.
 */
const _origin = typeof location !== 'undefined' ? location.origin : 'http://localhost';
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

/**
 * Resolve the business directory for multi-directory serving.
 * Build-time value wins, otherwise the workspace panel persists the
 * current repository root. Absent means requests carry no directory and
 * workspace-bound handlers reject them instead of guessing.
 */
export function resolveDirectory(): string | undefined {
	const fromEnv = import.meta.env.VITE_API_DIRECTORY as string | undefined;
	if (fromEnv && fromEnv.length > 0) return fromEnv;
	if (typeof localStorage !== 'undefined') {
		const fromStorage = localStorage.getItem('wf.directory');
		if (fromStorage && fromStorage.length > 0) return fromStorage;
	}
	return undefined;
}

/**
 * Validate a workspace directory spelling before persisting it: it must
 * be a non-empty absolute path. Server membership (allowlist, existence)
 * is checked by `fetchServerDirectories`, not here.
 */
export function validateDirectoryFormat(raw: string): string | undefined {
	const trimmed = raw.trim();
	if (trimmed.length === 0) return 'directory must not be empty';
	if (!trimmed.startsWith('/')) return 'directory must be absolute';
	return undefined;
}

/**
 * Persist the workspace directory after format validation. Empty clears
 * the stored value back to directory-less mode. Throws on invalid
 * spelling so callers surface the problem before any request fails.
 */
export function setDirectory(raw: string | undefined): string | undefined {
	if (typeof localStorage === 'undefined') return undefined;
	const trimmed = (raw ?? '').trim();
	if (trimmed.length === 0) {
		localStorage.removeItem('wf.directory');
		return undefined;
	}
	const problem = validateDirectoryFormat(trimmed);
	if (problem) throw new Error(problem);
	localStorage.setItem('wf.directory', trimmed);
	return trimmed;
}

/** Directory rejection kinds, so the UI can tell a typo from a server
 * that is not configured to serve the path. */
export type DirectoryProblem = 'not-found' | 'not-served' | 'validation';

/**
 * Classify a failed response as a directory problem when its envelope
 * code says so. `LOCATION_NOT_FOUND` means the path is wrong, `FORBIDDEN`
 * means the allowlist does not serve it.
 */
export function classifyDirectoryError(body: unknown, status: number): DirectoryProblem | undefined {
	const code =
		typeof body === 'object' && body !== null && 'error' in body
			? (body as { error?: { code?: unknown } }).error?.code
			: undefined;
	if (code === 'LOCATION_NOT_FOUND' || status === 404) return 'not-found';
	if (code === 'FORBIDDEN' || status === 403) return 'not-served';
	if (code === 'VALIDATION' || status === 400) return 'validation';
	return undefined;
}

/**
 * Read the server directory state (allowlist plus cached directories
 * with project identity) backing workspace pickers. Returns `undefined`
 * when the request itself fails, so pickers fall back to free input.
 */
export async function fetchServerDirectories(): Promise<
	{ allowedRoots: string[]; cached: { directory: string; project: string; projectId: string }[] } | undefined
> {
	const { data } = await request('GET', '/api/v1/system/directories');
	if (typeof data !== 'object' || data === null) return undefined;
	const view = (data as { data?: unknown }).data ?? data;
	if (typeof view !== 'object' || view === null) return undefined;
	const directories = view as {
		allowed_roots?: unknown;
		projects?: { directory?: unknown; project?: unknown; project_id?: unknown }[];
	};
	const allowedRoots = Array.isArray(directories.allowed_roots)
		? directories.allowed_roots.filter((root): root is string => typeof root === 'string')
		: [];
	const cached = Array.isArray(directories.projects)
		? directories.projects.flatMap((entry) =>
				typeof entry.directory === 'string' && typeof entry.project === 'string'
					? [
							{
								directory: entry.directory,
								project: entry.project,
								projectId: typeof entry.project_id === 'string' ? entry.project_id : '',
							},
						]
					: [],
				)
		: [];
	return { allowedRoots, cached };
}

export const client = createClient<paths>({
	baseUrl: API_BASE_URL,
	headers: { 'Content-Type': 'application/json' },
});

/**
 * Fetch a full-path endpoint and return the parsed JSON envelope together
 * with the HTTP status. Endpoints that bypass the typed client (raw file
 * download, upload, copy) share this helper in both the real and the
 * fixture-backed client so the services layer needs no mode branching.
 */
export async function request(
	method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
	path: string,
	init?: { body?: unknown },
): Promise<{ data?: unknown; error?: unknown; status: number }> {
	const key = resolveApiKey();
	const directory = resolveDirectory();
	const response = await fetch(`${API_BASE_URL}${path}`, {
		method,
		headers: {
			...(init?.body === undefined ? {} : { 'Content-Type': 'application/json' }),
			...(key ? { 'x-api-key': key } : {}),
			...(directory ? { 'x-wf-directory': directory } : {}),
		},
		body: init?.body === undefined ? undefined : JSON.stringify(init.body),
	});
	let body: unknown = null;
	try {
		body = await response.json();
	} catch {
		body = null;
	}
	if (!response.ok) {
		return { error: body, status: response.status };
	}
	return { data: body, status: response.status };
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
	const directory = resolveDirectory();
	const response = await fetch(`${API_BASE_URL}${path}`, {
		method: init?.method ?? 'GET',
		headers: {
			...(init?.body === undefined ? {} : { 'Content-Type': 'application/json' }),
			...(key ? { 'x-api-key': key } : {}),
			...(directory ? { 'x-wf-directory': directory } : {}),
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

/** Interceptor: inject x-api-key and x-wf-directory on every request. */
client.use({
	async onRequest({ request }) {
		const key = resolveApiKey();
		if (key) request.headers.set('x-api-key', key);
		const directory = resolveDirectory();
		if (directory) request.headers.set('x-wf-directory', directory);
		return request;
	},
});
