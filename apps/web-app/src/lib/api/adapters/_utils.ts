/**
 * Tiny helpers for defensive field access from API responses.
 *
 * Backend is still finishing OpenAPI type coverage (14% of endpoints return
 * `ApiEnvelope_Value`, i.e. `data: unknown`) so every adapter needs to cope
 * with missing fields.  Helpers here centralise the snake_case → camelCase
 * dance and null-coalescing.
 */

export type AnyRecord = Record<string, unknown>;

/** Best-effort snake_case → camelCase for a single key. */
export function toCamel(key: string): string {
	return key.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());
}

/** Read `obj[snake] ?? obj[camel] ?? fallback`. */
export function pick<T>(
	obj: AnyRecord | null | undefined,
	snakeOrCamel: string,
	fallback: T,
): T {
	if (!obj || typeof obj !== 'object') return fallback;
	if (snakeOrCamel in obj) return obj[snakeOrCamel] as T;
	const camel = toCamel(snakeOrCamel);
	if (camel in obj) return obj[camel] as T;
	return fallback;
}

/** Read `obj[snake]` as a string, coerce null/undefined/non-string to fallback. */
export function pickStr(
	obj: AnyRecord | null | undefined,
	snakeOrCamel: string,
	fallback = '',
): string {
	const v = pick<unknown>(obj, snakeOrCamel, undefined);
	if (typeof v === 'string') return v;
	if (v == null) return fallback;
	return String(v);
}

export function pickNum(
	obj: AnyRecord | null | undefined,
	snakeOrCamel: string,
	fallback: number | null = null,
): number | null {
	const v = pick<unknown>(obj, snakeOrCamel, undefined);
	if (typeof v === 'number') return v;
	if (typeof v === 'string' && v !== '') {
		const n = Number(v);
		return Number.isFinite(n) ? n : fallback;
	}
	return fallback;
}

export function pickBool(
	obj: AnyRecord | null | undefined,
	snakeOrCamel: string,
	fallback = false,
): boolean {
	const v = pick<unknown>(obj, snakeOrCamel, undefined);
	if (typeof v === 'boolean') return v;
	if (typeof v === 'string') return v === 'true' || v === '1';
	return fallback;
}

export function pickArr<T>(
	obj: AnyRecord | null | undefined,
	snakeOrCamel: string,
): T[] {
	const v = pick<unknown>(obj, snakeOrCamel, undefined);
	return Array.isArray(v) ? (v as T[]) : [];
}
