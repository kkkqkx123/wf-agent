/**
 * Execution detail route loader.
 *
 * The heaviest detail page — ExecutionInspector renders many sub-sections.
 * We only wire the endpoints the current UI actually consumes: base detail,
 * nodes list, and graph.  More sub-endpoints (audit, error-analysis,
 * checkpoints, context-snapshots, ...) can be added incrementally as the
 * Inspector component grows.
 */
import { client } from '$lib/api/client';
import { fallbackEnabled } from '$lib/api/fallback';
import { adaptExecution, adaptExecutionDetail } from '$lib/api/adapters/execution';
import { executionDetail as fixtureExecutionDetail } from '$lib/fixtures/executions';
import type { ExecutionDetail } from '$lib/types/models';

export interface PageData {
	detail: ExecutionDetail;
}

async function loadFromApi(id: string, fetch: typeof globalThis.fetch): Promise<PageData> {
	const [resp, nodesResp, graphResp] = await Promise.allSettled([
		client.GET('/api/v1/executions/{id}', {
			fetch,
			params: { path: { id } },
		}),
		client.GET('/api/v1/executions/{id}/nodes', {
			fetch,
			params: { path: { id } },
		}),
		client.GET('/api/v1/executions/{id}/graph', {
			fetch,
			params: { path: { id } },
		}),
	]);

	function extractData<T>(
		result: PromiseSettledResult<{ data?: unknown; error?: unknown }>,
	): T | undefined {
		if (result.status !== 'fulfilled') return undefined;
		const r = result.value as { data?: unknown; error?: unknown };
		if (r.error) return undefined;
		const env = r.data as { success?: boolean; data?: T } | undefined;
		if (env && typeof env.success === 'boolean') {
			return env.success ? env.data : undefined;
		}
		return env as T;
	}

	const base = extractData<unknown>(resp);
	const nodes = extractData<unknown[]>(nodesResp);
	const graph = extractData<unknown>(graphResp);

	if (!base) {
		if (import.meta.env.DEV) {
			console.warn('[api] executions/{id} primary endpoint failed, using fixture detail');
		}
		return { detail: fixtureExecutionDetail };
	}

	const detail = adaptExecutionDetail(base, { nodes, graph });
	void adaptExecution;
	return { detail };
}

export async function load({ params, fetch }): Promise<PageData> {
	const id = params.id ?? '';
	if (!id || fallbackEnabled()) {
		return { detail: fixtureExecutionDetail };
	}
	try {
		return await loadFromApi(id, fetch as typeof globalThis.fetch);
	} catch (err) {
		if (import.meta.env.DEV) {
			console.warn('[api] executions/{id} load failed, falling back to fixtures', err);
			return { detail: fixtureExecutionDetail };
		}
		throw err;
	}
}
