import { client } from '#lib/api/client';
import { ApiHttpError, call } from '#lib/api/envelope';
import type { components } from '#lib/api/schema';

type BackendListView = components['schemas']['BackendListView'];
type BackendEntryView = components['schemas']['BackendEntryView'];

/** One scoped backend row: the connected server's view of a remote. */
export interface BackendRow {
	name: string;
	url: string;
	reachable: boolean;
	latencyMs: number | null;
	version: string | null;
	apiVersion: string | null;
	tunnelRunning: boolean;
	interruptedReason: string | null;
}

/** Map one registry view row, tolerating absent optional fields. */
export function toBackendRow(row: BackendEntryView): BackendRow {
	return {
		name: row.name,
		url: row.url,
		reachable: row.status?.reachable ?? false,
		latencyMs: row.status?.latency_ms ?? null,
		version: row.status?.version ?? null,
		apiVersion: row.status?.api_version ?? null,
		tunnelRunning: row.tunnel?.running ?? false,
		interruptedReason: row.tunnel?.interrupted_reason ?? null,
	};
}

/** Map a list view to sorted rows. Nullish or malformed views yield no rows. */
export function toBackendRows(view: BackendListView | null | undefined): BackendRow[] {
	if (!view || !Array.isArray(view.backends)) return [];
	return view.backends.map(toBackendRow).sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Scoped read-only view of the connected server's remotes. Registration
 * stays a CLI operation; this only reads what the current connection
 * serves. Null when the backend domain is disabled or unreachable, so
 * callers render no backend section instead of failing the page. Other
 * failures (auth, corrupt payload) propagate.
 */
export async function listBackends(): Promise<BackendRow[] | null> {
	try {
		const view = await call<BackendListView>(client.GET('/api/v1/system/backends'));
		return toBackendRows(view);
	} catch (e) {
		if (e instanceof ApiHttpError && e.status === 503) return null;
		if (e instanceof TypeError) return null;
		throw e;
	}
}
