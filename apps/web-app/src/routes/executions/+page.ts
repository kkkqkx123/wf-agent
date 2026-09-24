/**
 * Executions list route loader.
 *
 *  - Real API: GET /api/v1/executions → PageView.items → adaptExecution
 *  - Fallback: fixtures when VITE_API_FALLBACK=true or request fails in dev
 */
import { client } from '$lib/api/client';
import { unwrapPage } from '$lib/api/envelope';
import { fallbackEnabled } from '$lib/api/fallback';
import { adaptExecution } from '$lib/api/adapters/execution';
import { executions as fixtureExecutions, executionDetail as fixtureExecutionDetail } from '$lib/fixtures/executions';
import { overviewMetrics } from '$lib/fixtures/insights';
import type { Execution, ExecutionDetail, Metric } from '$lib/types/models';

export interface PageData {
	executions: Execution[];
	executionDetail: ExecutionDetail;
	overviewMetrics: Metric[];
}

async function loadFromApi(fetch: typeof globalThis.fetch): Promise<PageData> {
	const resp = await client.GET('/api/v1/executions', { fetch });
	const page = unwrapPage<unknown>(resp);
	const executions = page.items.map(adaptExecution);

	let executionDetail: ExecutionDetail = fixtureExecutionDetail;
	const firstId = executions[0]?.id;
	if (firstId) {
		try {
			const detailResp = await client.GET('/api/v1/executions/{id}', {
				fetch,
				params: { path: { id: firstId } },
			});
			const raw = (detailResp.data as { success?: boolean; data?: unknown } | undefined)?.data;
			if (raw) {
				const { adaptExecutionDetail } = await import('$lib/api/adapters/execution');
				executionDetail = adaptExecutionDetail(raw) as ExecutionDetail;
			}
		} catch {
			// keep fixture detail shape
		}
	}

	return { executions, executionDetail, overviewMetrics };
}

export async function load({ fetch }): Promise<PageData> {
	if (fallbackEnabled()) {
		return { executions: fixtureExecutions, executionDetail: fixtureExecutionDetail, overviewMetrics };
	}
	try {
		return await loadFromApi(fetch as typeof globalThis.fetch);
	} catch (err) {
		if (import.meta.env.DEV) {
			console.warn('[api] executions load failed, falling back to fixtures', err);
			return { executions: fixtureExecutions, executionDetail: fixtureExecutionDetail, overviewMetrics };
		}
		throw err;
	}
}
