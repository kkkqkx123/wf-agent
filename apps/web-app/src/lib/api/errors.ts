/**
 * Normalised API error types consumed throughout the app.
 *
 * Backend returns `ApiEnvelope.error` (success=false) or HTTP non-2xx with
 * `ErrorResponse` body; openapi-fetch surfaces the latter on `response.error`
 * and the former nested inside `response.data.error`.  Both converge here.
 */

export type ApiErrorCode =
	| 'UNAUTHORIZED'
	| 'FORBIDDEN'
	| 'RATE_LIMITED'
	| 'NOT_FOUND'
	| 'INVALID_PARAMS'
	| 'SERVER_ERROR'
	| 'NETWORK_ERROR'
	| 'PARSE_ERROR';

export class ApiError extends Error {
	readonly code: ApiErrorCode;
	readonly httpStatus?: number;
	readonly backendCode?: string;

	constructor(
		code: ApiErrorCode,
		message: string,
		httpStatus?: number,
		backendCode?: string,
	) {
		super(message);
		this.name = 'ApiError';
		this.code = code;
		this.httpStatus = httpStatus;
		this.backendCode = backendCode;
	}
}

const CODE_MAP: Record<string, ApiErrorCode> = {
	unauthorized: 'UNAUTHORIZED',
	forbidden: 'FORBIDDEN',
	not_found: 'NOT_FOUND',
	invalid_params: 'INVALID_PARAMS',
	rate_limited: 'RATE_LIMITED',
};

function mapCode(code?: string): ApiErrorCode {
	if (!code) return 'SERVER_ERROR';
	return CODE_MAP[code] ?? 'SERVER_ERROR';
}

/**
 * Convert an openapi-fetch error envelope OR a backend envelope.error into
 * a single branchable ApiError.
 */
export function normalizeError(
	raw: unknown,
	httpStatus?: number,
): ApiError {
	if (raw instanceof ApiError) return raw;

	if (raw && typeof raw === 'object') {
		const obj = raw as Record<string, unknown>;
		// Prefer envelope { code, message } body
		const backendCode = (obj.code as string | undefined) ?? (obj.type as string | undefined);
		const message =
			(obj.message as string | undefined) ??
			(obj.error as string | undefined) ??
			(obj.detail as string | undefined) ??
			'Unknown API error';
		return new ApiError(mapCode(backendCode), message, httpStatus, backendCode);
	}

	if (typeof raw === 'string') {
		return new ApiError('SERVER_ERROR', raw, httpStatus);
	}

	return new ApiError('SERVER_ERROR', 'Unknown error', httpStatus);
}
