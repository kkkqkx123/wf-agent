/**
 * Mock router kernel for the fixture-backed preview API.
 *
 * Pure infrastructure: path matching, the method router, envelope /
 * pagination shapes and camelCase → snake_case helpers. Domain route
 * registrations live in `./routes/*` and are wired in `./registry.ts`.
 */

/** Convert one camelCase key to snake_case without double-underscores. */
function camelToSnake(s: string): string {
	// Only uppercase ASCII needs handling — fixture keys never have
	// leading underscores or unicode identifiers.
	return s.replace(/[A-Z]/g, (m) => '_' + m.toLowerCase());
}

/** Recursively convert camelCase object keys to snake_case. Arrays are
 *  recursed into; non-plain objects (dates, classes) are returned as-is. */
export function camelToSnakeDeep<T>(value: T): T {
	if (value === null || value === undefined) return value;
	if (Array.isArray(value)) {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		return value.map((v: any) => camelToSnakeDeep(v)) as T;
	}
	if (typeof value === 'object') {
		// Date, Map, Set etc. — preserve identity
		if (value instanceof Date) return value;
		const out: Record<string, unknown> = {};
		for (const [k, v] of Object.entries(value)) {
			out[camelToSnake(k)] = camelToSnakeDeep(v);
		}
		return out as T;
	}
	return value;
}

/** Wrap a list of DTOs in the backend's PageView shape. */
export function pageView<T>(items: T[], limit = 50, offset = 0) {
	return {
		items: items.slice(offset, offset + limit),
		has_more: offset + limit < items.length,
		limit,
		offset,
	};
}

/** Wrap a list of DTOs in the backend's CursorPageView shape. The cursor
 *  is the next numeric offset encoded as a string, matching the server. */
export function cursorPageView<T>(items: T[], limit = 50, cursor?: unknown) {
	const offset = Math.max(0, parseInt(String(cursor ?? '0'), 10) || 0);
	const window = items.slice(offset, offset + limit + 1);
	const hasMore = window.length > limit;
	window.length = Math.min(window.length, limit);
	return {
		items: window,
		limit,
		next_cursor: hasMore ? String(offset + limit) : null,
		has_more: hasMore,
	};
}

/** Best-effort extract of a query param from params.query. openapi-fetch
 *  passes `{ query: { limit, offset } }` for list calls. */
export function queryOf(
	params?: Record<string, unknown>,
): Record<string, unknown> {
	const q = params?.query;
	return q && typeof q === 'object' ? (q as Record<string, unknown>) : {};
}

/** Wrap a handler result the way the real backend envelope does, so the
 *  shared `envelope.ts` unwraps an identical shape in both projects. */
export function okEnvelope(data: unknown) {
	return { data: { success: true, data, error: null } };
}

/** Unrouted path: surface a 404 the services layer turns into an
 *  `ApiHttpError`, so missing fixture coverage renders as an explicit
 *  error state instead of a silent empty view. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function missingRoute(method: string, path: string): any {
	return {
		error: {
			success: false,
			data: null,
			error: {
				code: 'NOT_FOUND',
				message: `No mock route for ${method} ${path}`,
			},
		},
		response: { status: 404 },
	};
}

/**
 * Match `pathTemplate` (e.g. `/api/v1/workflows/{id}`) against a concrete
 * request path, returning captured params or null on miss.
 */
function matchPath(
	pathTemplate: string,
	actual: string,
): Record<string, string> | null {
	const regex = new RegExp(
		'^' + pathTemplate.replace(/\{[^}]+\}/g, '([^/]+)') + '$',
	);
	const names = [...pathTemplate.matchAll(/\{([^}]+)\}/g)].map((m) => m[1]);
	const m = regex.exec(actual);
	if (!m) return null;
	const out: Record<string, string> = {};
	names.forEach((n, i) => (out[n] = m[i + 1]));
	return out;
}

export type Handler = (
	params: Record<string, string>,
	query: Record<string, unknown>,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	body: any,
) => unknown;

/** Build a method router for GET/POST/PUT/DELETE. */
export function makeRouter() {
	// Method → [ [pattern, handler], ... ]
	const routes: Record<string, Array<[string, Handler]>> = {
		GET: [],
		POST: [],
		PUT: [],
		DELETE: [],
		PATCH: [],
	};

	function on(method: string, pattern: string, handler: Handler) {
		routes[method].push([pattern, handler]);
	}

	function dispatch(
		method: string,
		path: string,
		query: Record<string, unknown>,
		body: unknown,
	): unknown {
		for (const [pattern, handler] of routes[method] ?? []) {
			const params = matchPath(pattern, path);
			if (params !== null) {
				return handler(params, query, body);
			}
		}
		return undefined;
	}

	return { on, dispatch };
}

/** A router instance handed to the per-domain `register` functions. */
export type MockRouter = ReturnType<typeof makeRouter>;
