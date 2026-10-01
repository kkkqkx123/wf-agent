import { client, downloadFile } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type {
	QueryResult,
	AuditReport,
	ErrorAnalysis,
	Metric,
	PerfNode,
	Template,
	EventRecord,
	Dependency,
	Diagnostic,
} from '$lib/types/models';

type QueryRow = Record<string, string | number | null>;

interface PerfNodeDto {
	node?: string;
	calls?: number;
	avg_ms?: number;
	p95_ms?: number;
	share?: number;
}

function toPerfNode(d: PerfNodeDto): PerfNode {
	return {
		node: d.node ?? '',
		calls: d.calls ?? 0,
		avgMs: d.avg_ms ?? 0,
		p95Ms: d.p95_ms ?? 0,
		share: d.share ?? 0,
	};
}

/**
 * Run an ad-hoc filter/query against the backend. The actual body shape
 * matches QueryBody from schema.d.ts: expressions + filters + limit + offset.
 */
export async function runQuery(params: {
	filters?: Record<string, unknown>;
	expressions?: Record<string, unknown>[];
	limit?: number;
	offset?: number;
}): Promise<QueryResult> {
	const body = {
		filters: params.filters ?? null,
		expressions: params.expressions ?? [],
		limit: params.limit ?? 50,
		offset: params.offset ?? 0,
	};
	const data = await call<unknown>(client.POST('/api/v1/query', { body }));

	if (data && typeof data === 'object') {
		const d = data as Record<string, unknown>;
		const columns = (d.columns as string[] | undefined) ?? [];
		const rowsRaw = (d.rows as unknown[] | undefined) ?? [];
		const rows = rowsRaw.map((r) => {
			const obj: Record<string, string | number | null> = {};
			if (r && typeof r === 'object') {
				for (const [k, v] of Object.entries(r as object)) {
					if (v === null || typeof v === 'string' || typeof v === 'number') {
						obj[k] = v;
					} else if (typeof v === 'boolean') {
						obj[k] = String(v);
					} else {
						obj[k] = JSON.stringify(v);
					}
				}
			}
			return obj;
		}) as QueryRow[];
		const elapsedMs = Number(d.elapsed_ms ?? d.elapsedMs ?? 0);
		const truncated = Boolean(d.truncated);
		return { columns, rows, elapsedMs, truncated };
	}
	return { columns: [], rows: [], elapsedMs: 0, truncated: false };
}

/** Overview metric cards backed by the analysis stats aggregator. */
export async function getOverviewMetrics(): Promise<Metric[]> {
	const data = await call<unknown>(client.GET('/api/v1/analysis/stats'));
	const rows: Metric[] = [];
	if (data && typeof data === 'object' && !Array.isArray(data)) {
		for (const [label, value] of Object.entries(
			data as Record<string, unknown>,
		)) {
			if (typeof value === 'number' || typeof value === 'string') {
				rows.push({ label, value: String(value), tone: 'info' });
			}
			if (rows.length >= 4) break;
		}
	}
	return rows;
}

/** Template library served through the template registry routes. */
export async function listInsightTemplates(): Promise<Template[]> {
	const { listTemplates } = await import('$lib/services/templates');
	return listTemplates({ kind: 'all' });
}

/**
 * Last query result placeholder. Live ad-hoc queries go through runQuery;
 * the Query tab starts empty until the user runs a statement.
 */
export async function getQueryResult(): Promise<QueryResult> {
	return { columns: [], rows: [], elapsedMs: 0, truncated: false };
}

/**
 * Global error analysis.
 *
 * NOTE: The backend exposes per-execution analysis
 * (`GET /api/v1/executions/{id}/error-analysis/advanced`) but no global
 * aggregator. Returns an empty list until the aggregator lands.
 */
export async function listErrorAnalyses(): Promise<ErrorAnalysis[]> {
	return [];
}

/**
 * Global audit reports.
 *
 * NOTE: The backend only exposes per-execution audit endpoints
 * (`GET /api/v1/executions/{id}/audit/report`), with no global aggregator yet.
 * Returns an empty list until that aggregator lands.
 */
export async function listInsightAuditReports(): Promise<AuditReport[]> {
	return [];
}

/**
 * Fetch performance node list via the top-node aggregator.
 */
export async function listPerformanceNodes(): Promise<PerfNode[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/analysis/stats/top-node-types'),
	);
	if (Array.isArray(data)) {
		return (data as PerfNodeDto[]).map(toPerfNode);
	}
	return [];
}

interface InsightEventDto {
	id?: string;
	type?: string;
	source?: string;
	at?: string;
	timestamp?: string;
	execution_id?: string | null;
	executionId?: string | null;
	payload?: string | Record<string, unknown>;
}

interface InsightDependencyDto {
	id?: string;
	caller?: string;
	callee?: string;
	kind?: string;
	calls?: number;
	call_count?: number;
	last_called_at?: string;
	lastCalledAt?: string;
}

interface InsightHealthDto {
	ready?: boolean;
	storage?: string;
	persistence?: Record<string, unknown>;
}

function toInsightEvent(d: InsightEventDto): EventRecord {
	const payload = d.payload;
	return {
		id: d.id ?? '',
		type: d.type ?? '',
		source: d.source ?? '',
		at: d.at ?? d.timestamp ?? '',
		executionId: d.execution_id ?? d.executionId ?? null,
		payload:
			typeof payload === 'string'
				? payload
				: payload
					? JSON.stringify(payload)
					: '',
	};
}

function toInsightDependency(d: InsightDependencyDto): Dependency {
	return {
		id: d.id ?? `${d.caller ?? ''}->${d.callee ?? ''}`,
		caller: d.caller ?? '',
		callee: d.callee ?? '',
		kind: d.kind ?? '',
		calls: d.calls ?? d.call_count ?? 0,
		lastCalledAt: d.last_called_at ?? d.lastCalledAt ?? '',
	};
}

/** Event stream backed by the event store. */
export async function listInsightEvents(): Promise<EventRecord[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/events', { params: { query: { limit: 100 } } }),
	);
	return extractPage<InsightEventDto>(data).items.map(toInsightEvent);
}

/** Dependency audit backed by the dependency auditor. */
export async function listInsightDependencies(): Promise<Dependency[]> {
	const data = await call<unknown>(client.GET('/api/v1/dependencies/audit'));
	const items = Array.isArray(data)
		? (data as InsightDependencyDto[])
		: extractPage<InsightDependencyDto>(data).items;
	return items.map(toInsightDependency);
}

/** Diagnostics aggregated from health and storage probes. */
export async function listInsightDiagnostics(): Promise<Diagnostic[]> {
	const results = await Promise.allSettled([
		call<InsightHealthDto>(client.GET('/health')),
		call<unknown>(client.GET('/api/v1/storage/diagnose')),
		call<unknown>(client.GET('/api/v1/storage/stats')),
	]);
	const diagnostics: Diagnostic[] = [];
	if (results[0].status === 'fulfilled') {
		const health = results[0].value;
		diagnostics.push({
			name: 'ready',
			status: health?.ready ? 'healthy' : 'degraded',
			value: String(health?.ready),
			detail: health?.storage ?? '',
		});
		if (health?.persistence && typeof health.persistence === 'object') {
			for (const [name, value] of Object.entries(health.persistence)) {
				diagnostics.push({
					name: `persistence.${name}`,
					status: 'info',
					value: String(value),
					detail: '',
				});
			}
		}
	}
	if (results[1].status === 'fulfilled' && Array.isArray(results[1].value)) {
		for (const item of results[1].value as Array<{
			name?: string;
			status?: string;
			value?: string | number;
			detail?: string;
		}>) {
			diagnostics.push({
				name: item.name ?? '',
				status: item.status ?? 'info',
				value: String(item.value ?? ''),
				detail: item.detail ?? '',
			});
		}
	}
	if (
		results[2].status === 'fulfilled' &&
		results[2].value &&
		typeof results[2].value === 'object'
	) {
		diagnostics.push({
			name: 'storage.stats',
			status: 'info',
			value: JSON.stringify(results[2].value).slice(0, 120),
			detail: '',
		});
	}
	return diagnostics;
}

/** Run a query export and trigger a browser download of the result file. */
export async function exportQuery(params: {
	expressions: Record<string, unknown>[];
	format?: 'json' | 'csv';
	limit?: number;
}): Promise<void> {
	const format = params.format ?? 'csv';
	await downloadFile(
		'/api/v1/query/export?download=true',
		`query-export.${format}`,
		{
			method: 'POST',
			body: {
				expressions: params.expressions,
				format,
				limit: params.limit,
			},
		},
	);
}
