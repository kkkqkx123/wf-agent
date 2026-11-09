/**
 * Fixture-backed replacement for openapi-fetch's client.
 *
 * Preview mode never talks to a real wf-server. Every call to
 * `client.GET` / `client.POST` below resolves instantly against the
 * hard-coded sample data in `#lib/fixtures`. The return shape matches
 * openapi-fetch ({ data, error, response }) so `envelope.ts` and the
 * services layer stay completely unchanged.
 *
 * This file is selected by the `#lib/api/client` alias in `vite.config.ts`
 * and `tsconfig.preview.json` when running with `--mode preview`; every
 * other mode uses the real client (`client.ts`).
 *
 * The mock internals live under `./mock/`: the router kernel
 * (`mock/router.ts`), per-domain route registrations (`mock/routes/*`,
 * wired in registration order by `mock/registry.ts`), wire adapters
 * (`mock/wire.ts`) and the fixture SSE stream (`mock/event-stream.ts`).
 * This facade keeps the public surface (`client`, `request`,
 * `downloadFile`, `API_BASE_URL`, `resolveApiKey`, `resolveDirectory`,
 * `validateDirectoryFormat`, `setDirectory`, `classifyDirectoryError`,
 * `fetchServerDirectories`) that shared code
 * imports, so the two clients stay drop-in interchangeable.
 */

import { makeRouter, missingRoute, okEnvelope, queryOf } from './mock/router';
import { registerAllRoutes } from './mock/registry';
import './mock/event-stream';
import { installMockPtySocket } from './mock/pty-socket';

installMockPtySocket();

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

function dispatchEnvelope(
	method: string,
	path: string,
	query: Record<string, unknown>,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	body: any,
): { data?: unknown; error?: unknown; response?: { status: number }; status: number } {
	const data = router.dispatch(method, path, query, body);
	if (data === undefined) return missingRoute(method, path);
	// Route handlers may return a MockRouteError (e.g. an fs write conflict);
	// surface it as a genuine error envelope so `call()` throws ApiHttpError.
	const candidate = data as {
		__mockError?: boolean;
		status?: number;
		code?: string;
		message?: string;
	};
	if (candidate && candidate.__mockError === true) {
		return {
			error: {
				success: false,
				data: null,
				error: {
					code: candidate.code ?? 'HTTP_ERROR',
					message: candidate.message ?? 'Preview error',
				},
			},
			response: { status: candidate.status ?? 400 },
			status: candidate.status ?? 400,
		};
	}
	return okEnvelope(data);
}

// ---------------------------------------------------------------------------
// Public client object — mirrors openapi-fetch surface used by services
// ---------------------------------------------------------------------------

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Mirror openapi-fetch path substitution so `{id}` templates resolve
 *  from `params.path` before the mock router matches. Unmatched keys stay
 *  literal so unrouted paths still surface as explicit 404s. */
function substitutePath(path: string, params?: { path?: Record<string, unknown> }): string {
	const values = params?.path;
	if (!values) return path;
	return path.replace(/\{([^}]+)\}/g, (match, key: string) => {
		const value = values[key];
		return value === undefined || value === null ? match : encodeURIComponent(String(value));
	});
}

// Route handlers may return a MockRouteError (e.g. unknown PTY session);
// surface it as a genuine error envelope so `call()` throws ApiHttpError
// instead of handing the marker object to services as a fake payload.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function toPreviewResult(method: string, path: string, data: any): any {
	if (data === undefined) return missingRoute(method, path);
	const candidate = data as {
		__mockError?: boolean;
		status?: number;
		code?: string;
		message?: string;
	};
	if (candidate && candidate.__mockError === true) {
		return {
			error: {
				success: false,
				data: null,
				error: {
					code: candidate.code ?? 'HTTP_ERROR',
					message: candidate.message ?? 'Preview error',
				},
			},
			response: { status: candidate.status ?? 400 },
			status: candidate.status ?? 400,
		};
	}
	return okEnvelope(data);
}

export const client: AnyClient = {
	// openapi-fetch middleware hooks — no-ops in preview
	use() {},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async GET(path: string, opts?: any): Promise<any> {
		await delay(30); // simulate a tiny network latency
		const query = queryOf(opts?.params);
		const data = router.dispatch('GET', substitutePath(path, opts?.params), query, undefined);
		return toPreviewResult('GET', path, data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async POST(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const body = opts?.body;
		const data = router.dispatch('POST', substitutePath(path, opts?.params), query, body);
		return toPreviewResult('POST', path, data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async PUT(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const body = opts?.body;
		const data = router.dispatch('PUT', substitutePath(path, opts?.params), query, body);
		return toPreviewResult('PUT', path, data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async DELETE(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const data = router.dispatch('DELETE', substitutePath(path, opts?.params), query, undefined);
		return toPreviewResult('DELETE', path, data);
	},

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	async PATCH(path: string, opts?: any): Promise<any> {
		await delay(30);
		const query = queryOf(opts?.params);
		const body = opts?.body;
		const data = router.dispatch('PATCH', substitutePath(path, opts?.params), query, body);
		return toPreviewResult('PATCH', path, data);
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
): Promise<{ data?: unknown; error?: unknown; status: number }> {
	await delay(30);
	const query = queryOf(opts?.params);
	const body = opts?.body;
	return dispatchEnvelope(method, substitutePath(path, opts?.params), query, body);
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

// ---------------------------------------------------------------------------
// Workspace identity — shared with the real client so tab labels, recents
// and the address bar behave identically in preview. Fixture routes ignore
// the directory header, so resolution stays side-effect free for data.
// ---------------------------------------------------------------------------
export {
	DIRECTORY_PARAM,
	TAB_LABEL_PARAM,
	HOST_PARAM,
	resolveDirectory,
	pinnedDirectory,
	validateDirectoryFormat,
	normalizeDirectory,
	setDirectory,
	onDirectoryChange,
	onWorkspaceLabelChange,
	readRecentDirectories,
	pushRecentDirectory,
	directoryNamespace,
	shortWorkspaceName,
	shortHostPrefix,
	resolveTabName,
	setTabName,
	resolveHostPrefix,
	setHostPrefix,
	buildTabLabel,
	resolveTabLabel,
	applyDocumentTitle,
} from './workspace';
export type { RecentDirectory } from './workspace';

export type DirectoryProblem = 'not-found' | 'not-served' | 'validation';

export function classifyDirectoryError(
	_body: unknown,
	_status: number,
): DirectoryProblem | undefined {
	return undefined;
}

export async function fetchServerDirectories(): Promise<
	| { allowedRoots: string[]; cached: { directory: string; project: string; projectId: string }[] }
	| undefined
> {
	return { allowedRoots: [], cached: [] };
}
