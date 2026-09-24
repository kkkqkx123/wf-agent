/**
 * Helpers for peeling the uniform `ApiEnvelope` the backend wraps every
 * response in.  openapi-typescript already models these envelopes (e.g.
 * `components["schemas"]["ApiEnvelope_PageView_Value"]`) but callers would
 * otherwise have to spell `response.data?.data?.items` every single time.
 */
import { ApiError, normalizeError } from './errors';

type EnvelopeLike<T> = {
	success: boolean;
	data?: T;
	error?: { code: string; message: string } | null;
};

/**
 * Unwrap an openapi-fetch response, throwing on any error:
 *   - HTTP non-2xx → ApiError derived from `response.error` / status
 *   - success=false → ApiError derived from envelope.error
 *   - otherwise returns the inner envelope.data
 */
export function unwrap<T>(response: { data?: unknown; error?: unknown }): T {
	// openapi-fetch HTTP-level failure (e.g. 401 without JSON body)
	if (response.error) {
		throw normalizeError(response.error);
	}

	const raw = response.data as EnvelopeLike<T> | T | undefined;
	if (raw && typeof raw === 'object' && 'success' in raw && typeof (raw as EnvelopeLike<T>).success === 'boolean') {
		const env = raw as EnvelopeLike<T>;
		if (!env.success) {
			const inner = env.error ?? null;
			throw normalizeError(inner);
		}
		return (env.data ?? (null as unknown)) as T;
	}

	// Non-enveloped response (rare — e.g. health probe, raw file)
	return raw as T;
}

/**
 * Unwrap a paginated list endpoint.
 *
 * Backend uses `PageView { items, limit, offset, has_more }` for paginated
 * lists and `CappedView { items, total, truncated }` for hard-capped ones.
 * The raw types live under `components.schemas` — this function normalises
 * both into a single shape callers can iterate.
 */
export function unwrapPage<T>(response: { data?: unknown; error?: unknown }): {
	items: T[];
	limit?: number;
	offset?: number;
	hasMore: boolean;
	total?: number;
} {
	const envelopeData = unwrap<unknown>(response);
	if (!envelopeData || typeof envelopeData !== 'object') {
		return { items: [], hasMore: false };
	}

	const obj = envelopeData as Record<string, unknown>;

	// PageView
	if ('items' in obj && ('has_more' in obj || 'hasMore' in obj)) {
		const items = (obj.items as T[]) ?? [];
		const hasMore =
			(obj.has_more as boolean | undefined) ??
			(obj.hasMore as boolean | undefined) ??
			false;
		return {
			items,
			limit: obj.limit as number | undefined,
			offset: obj.offset as number | undefined,
			hasMore,
			total: obj.total as number | undefined,
		};
	}

	// CappedView (checkpoints, conversations, ...)
	if ('items' in obj && ('truncated' in obj || 'paths' in obj)) {
		const items = (obj.items as T[]) ?? [];
		const truncated = obj.truncated as boolean | undefined;
		return {
			items,
			hasMore: truncated ?? false,
			total: obj.total as number | undefined,
		};
	}

	// Plain array fallback
	if (Array.isArray(envelopeData)) {
		return { items: envelopeData as T[], hasMore: false };
	}

	return { items: [], hasMore: false };
}

/** Re-export so adapters and load functions can `import { ApiError }`. */
export { ApiError };
