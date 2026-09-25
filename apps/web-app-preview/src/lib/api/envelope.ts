import type { components } from './schema';

type ApiErrorBody = components['schemas']['ApiErrorBody'];

/** Thrown when the backend returns success:false or an HTTP error. */
export class ApiHttpError extends Error {
	public readonly status: number;
	public readonly code: string;
	public readonly envelopeError?: ApiErrorBody;

	constructor(status: number, code: string, message: string, envelopeError?: ApiErrorBody) {
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
 * Await an openapi-fetch client call, extract `data` from the envelope,
 * and throw a typed ApiHttpError on any failure path.
 *
 * openapi-fetch already unwraps one level of the envelope (its `data`
 * property is the envelope's `data` field), but we still need to
 * handle both the HTTP error case and the `success:false` envelope case.
 */
export async function call<T>(
	promise: Promise<{ data?: unknown; error?: unknown }>,
): Promise<T> {
	const res = await promise;

	// HTTP error branch — openapi-fetch populates error on 4xx/5xx
	if (res.error) {
		const e = res.error as { status?: number; body?: ApiErrorBody; message?: string };
		const body = e.body;
		const message = body?.message ?? e.message ?? 'HTTP request failed';
		throw new ApiHttpError(e.status ?? 0, body?.code ?? 'HTTP_ERROR', message, body);
	}

	// Envelope-level success:false — backend returned business error with 2xx
	const data = res.data as
		| { success: boolean; error?: ApiErrorBody; data?: T }
		| undefined;
	if (data && typeof data === 'object' && 'success' in data) {
		if (data.success === false) {
			const body = data.error;
			const message = body?.message ?? 'Backend business error';
			throw new ApiHttpError(0, body?.code ?? 'BUSINESS_ERROR', message, body);
		}
		// Envelope is transparent; openapi-fetch already returned the inner data
		return (data as unknown) as T;
	}

	// No envelope wrapper — raw value returned
	return data as T;
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
