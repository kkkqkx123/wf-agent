import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { EventRecord, Dependency, Diagnostic } from '$lib/types/models';

interface EventDto {
	id?: string;
	type?: string;
	source?: string;
	at?: string;
	timestamp?: string;
	execution_id?: string | null;
	executionId?: string | null;
	payload?: string | Record<string, unknown>;
}

interface DependencyDto {
	id?: string;
	caller?: string;
	callee?: string;
	kind?: string;
	calls?: number;
	call_count?: number;
	last_called_at?: string;
	lastCalledAt?: string;
}

interface HealthDto {
	ready?: boolean;
	storage?: string;
	persistence?: Record<string, unknown>;
}

interface StorageDiagnoseDto {
	name?: string;
	status?: string;
	value?: string | number;
	detail?: string;
}

function toEvent(d: EventDto): EventRecord {
	const payloadRaw = d.payload;
	const payloadStr =
		typeof payloadRaw === 'string'
			? payloadRaw
			: payloadRaw
				? JSON.stringify(payloadRaw)
				: '';
	return {
		id: d.id ?? '',
		type: d.type ?? '',
		source: d.source ?? '',
		at: d.at ?? d.timestamp ?? '',
		executionId: d.execution_id ?? d.executionId ?? null,
		payload: payloadStr,
	};
}

function toDependency(d: DependencyDto): Dependency {
	return {
		id: d.id ?? `${d.caller ?? ''}->${d.callee ?? ''}`,
		caller: d.caller ?? '',
		callee: d.callee ?? '',
		kind: d.kind ?? '',
		calls: d.calls ?? d.call_count ?? 0,
		lastCalledAt: d.last_called_at ?? d.lastCalledAt ?? '',
	};
}

function toDiagnosticFromHealth(h: HealthDto): Diagnostic[] {
	const list: Diagnostic[] = [];
	list.push({
		name: 'ready',
		status: h.ready ? 'healthy' : 'degraded',
		value: String(h.ready),
		detail: h.storage ?? '',
	});
	if (h.persistence && typeof h.persistence === 'object') {
		for (const [name, value] of Object.entries(h.persistence)) {
			list.push({
				name: `persistence.${name}`,
				status: 'info',
				value: String(value),
				detail: '',
			});
		}
	}
	return list;
}

export async function listEvents(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<EventRecord>> {
	const data = await call<unknown>(
		client.GET('/api/v1/events', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<EventDto>(data);
	return { ...page, items: page.items.map(toEvent) };
}

export async function searchEvents(
	query: string,
): Promise<PageResult<EventRecord>> {
	const data = await call<unknown>(
		client.GET('/api/v1/events/search', {
			params: { query: { q: query, limit: 50 } },
		}),
	);
	const page = extractPage<EventDto>(data);
	return { ...page, items: page.items.map(toEvent) };
}

export async function getEventSize(): Promise<number> {
	const data = await call<unknown>(client.GET('/api/v1/events/size'));
	if (typeof data === 'number') return data;
	if (data && typeof data === 'object' && 'size' in (data as object)) {
		return Number((data as { size?: number }).size ?? 0);
	}
	return 0;
}

/** Purges the whole event store (the only deletion granularity the API offers). */
export async function deleteAllEvents(): Promise<void> {
	await call<unknown>(
		client.DELETE('/api/v1/events', {
			params: { query: { force: true } },
		}),
	);
}

/** Liveness probe for the sidebar indicator. */
export async function getHealth(): Promise<boolean> {
	try {
		const data = await call<{ ready?: boolean }>(client.GET('/health'));
		return data?.ready !== false;
	} catch {
		return false;
	}
}

export async function listDependencies(): Promise<Dependency[]> {
	const data = await call<unknown>(client.GET('/api/v1/dependencies/audit'));
	if (Array.isArray(data)) {
		return (data as DependencyDto[]).map(toDependency);
	}
	return [];
}

export async function getDiagnostics(): Promise<Diagnostic[]> {
	const results = await Promise.allSettled([
		call<HealthDto>(client.GET('/health')),
		call<unknown>(client.GET('/api/v1/storage/diagnose')),
		call<unknown>(client.GET('/api/v1/storage/stats')),
	]);

	const diagnostics: Diagnostic[] = [];

	// /health
	if (results[0].status === 'fulfilled') {
		diagnostics.push(...toDiagnosticFromHealth(results[0].value));
	} else {
		diagnostics.push({
			name: 'health',
			status: 'unknown',
			value: 'unreachable',
			detail: 'Failed to fetch /health endpoint',
		});
	}

	// /storage/diagnose returns a list
	if (results[1].status === 'fulfilled') {
		const data = results[1].value;
		if (Array.isArray(data)) {
			for (const item of data as StorageDiagnoseDto[]) {
				diagnostics.push({
					name: item.name ?? '',
					status: item.status ?? 'info',
					value: String(item.value ?? ''),
					detail: item.detail ?? '',
				});
			}
		}
	}

	// /storage/stats — surface as a single aggregated entry
	if (results[2].status === 'fulfilled') {
		const data = results[2].value;
		if (data && typeof data === 'object') {
			diagnostics.push({
				name: 'storage.stats',
				status: 'info',
				value: JSON.stringify(data).slice(0, 120),
				detail: '',
			});
		}
	}

	return diagnostics;
}
