/**
 * Workflow detail route loader.
 *
 * Calls GET /api/v1/workflows/{id} and augments with graph + versions
 * sub-endpoints in parallel.  All fail-open so a single missing sub-endpoint
 * does not blank the page.
 */
import { client } from '$lib/api/client';
import { fallbackEnabled } from '$lib/api/fallback';
import { adaptWorkflow, adaptWorkflowDetail } from '$lib/api/adapters/workflow';
import { workflowDetail as fixtureWorkflowDetail } from '$lib/fixtures/workflows';
import type { WorkflowDetail } from '$lib/types/models';

export interface PageData {
	detail: WorkflowDetail;
}

async function loadFromApi(id: string, fetch: typeof globalThis.fetch): Promise<PageData> {
	const [resp, versionsResp] = await Promise.allSettled([
		client.GET('/api/v1/workflows/{id}', {
			fetch,
			params: { path: { id } },
		}),
		client.GET('/api/v1/workflows/{id}/versions', {
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
	const versions = extractData<unknown[]>(versionsResp);

	if (!base) {
		if (import.meta.env.DEV) {
			console.warn('[api] workflows/{id} primary endpoint failed, using fixture detail');
		}
		return { detail: fixtureWorkflowDetail };
	}

	const detail = adaptWorkflowDetail(base, { versions });
	void adaptWorkflow;
	return { detail };
}

export async function load({ params, fetch }): Promise<PageData> {
	const id = params.id ?? '';
	if (!id || fallbackEnabled()) {
		return { detail: fixtureWorkflowDetail };
	}
	try {
		return await loadFromApi(id, fetch as typeof globalThis.fetch);
	} catch (err) {
		if (import.meta.env.DEV) {
			console.warn('[api] workflows/{id} load failed, falling back to fixtures', err);
			return { detail: fixtureWorkflowDetail };
		}
		throw err;
	}
}
