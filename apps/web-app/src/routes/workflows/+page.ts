/**
 * Workflows list route loader.
 *
 *  - Real API: GET /api/v1/workflows/summaries → PageView.items → adaptWorkflow
 *  - Fallback: fixtures when VITE_API_FALLBACK=true or request fails in dev
 */
import { client } from '$lib/api/client';
import { unwrapPage } from '$lib/api/envelope';
import { fallbackEnabled } from '$lib/api/fallback';
import { adaptWorkflow, adaptWorkflowDetail } from '$lib/api/adapters/workflow';
import { workflows as fixtureWorkflows, workflowDetail as fixtureWorkflowDetail } from '$lib/fixtures/workflows';
import type { Workflow, WorkflowDetail } from '$lib/types/models';

export interface PageData {
	workflows: Workflow[];
	workflowDetail: WorkflowDetail;
}

async function loadFromApi(fetch: typeof globalThis.fetch): Promise<PageData> {
	const resp = await client.GET('/api/v1/workflows/summaries', { fetch });
	const page = unwrapPage<unknown>(resp);
	const workflows = page.items.map(adaptWorkflow);

	let workflowDetail: WorkflowDetail = fixtureWorkflowDetail;
	const firstId = workflows[0]?.id;
	if (firstId) {
		try {
			const detailResp = await client.GET('/api/v1/workflows/{id}', {
				fetch,
				params: { path: { id: firstId } },
			});
			const raw = (detailResp.data as { success?: boolean; data?: unknown } | undefined)?.data;
			if (raw) {
				workflowDetail = adaptWorkflowDetail(raw);
			}
		} catch {
			// keep fixture detail shape
		}
	}

	return { workflows, workflowDetail };
}

export async function load({ fetch }): Promise<PageData> {
	if (fallbackEnabled()) {
		return { workflows: fixtureWorkflows, workflowDetail: fixtureWorkflowDetail };
	}
	try {
		return await loadFromApi(fetch as typeof globalThis.fetch);
	} catch (err) {
		if (import.meta.env.DEV) {
			console.warn('[api] workflows load failed, falling back to fixtures', err);
			return { workflows: fixtureWorkflows, workflowDetail: fixtureWorkflowDetail };
		}
		throw err;
	}
}
