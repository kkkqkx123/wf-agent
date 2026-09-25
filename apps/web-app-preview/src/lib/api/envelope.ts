import type { components } from './schema';

type ApiErrorBody = components['schemas']['ApiErrorBody'];

/** Thrown when the backend returns success:false or an HTTP error. */
export class ApiHttpError extends Error {
	public readonly status: number;
	public readonly code: string;
	public readonly envelopeError?: ApiErrorBody;

	constructor(
		status: number,
		code: string,
		message: string,
		envelopeError?: ApiErrorBody,
	) {
		super(message);
		this.name = 'ApiHttpError';
		this.status = status;
		this.code = code;
		this.envelopeError = envelopeError;
	}
}

/** One page of a paginated list response. */
export interface PageResult<T> {
	items: T[];
	hasMore: boolean;
	limit: number;
	offset: number;
}

/** Capped list view with truncation flag. */
export interface CappedResult<T> {
	items: T[];
	total: number;
	truncated: boolean;
}

/**
 * Await an openapi-fetch client call and return the payload.
 *
 * openapi-fetch resolves to `{ response, data }` on 2xx and to
 * `{ response, error }` otherwise, where `error` is the `ErrorResponse` body.
 * Success payloads are wrapped in `{ success, data }`, so the inner `data`
 * is what the services actually consume.
 */
export async function call<T>(
	promise: Promise<{
		data?: unknown;
		error?: unknown;
		response?: Response;
	}>,
): Promise<T> {
	const res = await promise;

	if (res.error) {
		const body = (res.error as { error?: ApiErrorBody }).error;
		throw new ApiHttpError(
			res.response?.status ?? 0,
			body?.code ?? 'HTTP_ERROR',
			body?.message ?? 'HTTP request failed',
			body,
		);
	}

	return (res.data as { data?: T } | undefined)?.data as T;
}

/**
 * Extract a PageView from the call result. Accepts either a bare PageView
 * or an already-unwrapped items array (when the backend omits pagination).
 */
export function extractPage<T>(data: unknown): PageResult<T> {
	if (!data || typeof data !== 'object') {
		return { items: [], hasMore: false, limit: 0, offset: 0 };
	}
	const d = data as Record<string, unknown>;
	const items = (Array.isArray(d.items) ? d.items : []) as T[];
	return {
		items,
		hasMore: (d.has_more as boolean | undefined) ?? false,
		limit: (d.limit as number | undefined) ?? items.length,
		offset: (d.offset as number | undefined) ?? 0,
	};
}

/** Extract a CappedView (chain / timeline) from the call result. */
export function extractCapped<T>(data: unknown): CappedResult<T> {
	if (!data || typeof data !== 'object') {
		return { items: [], total: 0, truncated: false };
	}
	const d = data as Record<string, unknown>;
	return {
		items: (Array.isArray(d.items) ? d.items : []) as T[],
		total: (d.total as number | undefined) ?? 0,
		truncated: (d.truncated as boolean | undefined) ?? false,
	};
}
