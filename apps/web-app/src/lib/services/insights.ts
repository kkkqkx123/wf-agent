import { client, request, downloadFile } from '$lib/api/client';
import { call } from '$lib/api/envelope';
import type {
	QueryResult,
	AuditReport,
	ErrorAnalysis,
	PerfNode,
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
	const data = await call<unknown>(request('POST', '/api/v1/query', { body }));

	if (data && typeof data === 'object') {
		const d = data as Record<string, unknown>;
		const columns = (d.columns as string[]) ?? [];
		const rowsRaw = ((d.rows ?? []) as unknown[]) ?? [];
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

/**
 * Fetch global audit reports.
 *
 * NOTE: The backend currently only exposes per-execution audit endpoints
 * (`GET /api/v1/executions/{id}/audit/report`). There is no global aggregator
 * yet. Returns an empty list — a full implementation requires either a backend
 * aggregator or fan-out across executions, deferred to Batch 2.
 */
export async function listAuditReports(): Promise<AuditReport[]> {
	return [];
}

/**
 * Fetch global error analysis.
 *
 * NOTE: Same constraint as audit — backend exposes per-execution analysis
 * (`GET /api/v1/executions/{id}/error-analysis/advanced`) but no global
 * aggregator. Returns an empty list until the aggregator lands.
 */
export async function listErrorAnalyses(): Promise<ErrorAnalysis[]> {
	return [];
}

/**
 * Fetch performance node list via the top-node aggregator.
 */
export async function listPerformanceNodes(): Promise<PerfNode[]> {
	try {
		const data = await call<unknown>(
			client.GET('/api/v1/analysis/stats/top-node-types'),
		);
		if (Array.isArray(data)) {
			return (data as PerfNodeDto[]).map(toPerfNode);
		}
	} catch {
		// Endpoint shape differs or transient error.
	}
	return [];
}

/** Aggregate usage stats across the system. */
export async function getAnalysisStats(): Promise<Record<string, unknown>> {
	const data = await call<unknown>(client.GET('/api/v1/analysis/stats'));
	return (data as Record<string, unknown>) ?? {};
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
