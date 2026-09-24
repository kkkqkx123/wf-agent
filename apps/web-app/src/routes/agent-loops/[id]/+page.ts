/**
 * Agent-loop detail route loader.
 *
 * Parallels the inspector pane in the list view but hydrates ALL sub-endpoints
 * the full-detail tab bar renders: messages, variables, graph, iteration
 * history, and checkpoints.  Any single sub-endpoint failing degrades gracefully
 * rather than blocking the page.
 */
import { client } from '$lib/api/client';
import { fallbackEnabled } from '$lib/api/fallback';
import { adaptAgentLoop, adaptAgentLoopDetail } from '$lib/api/adapters/agent';
import { loopDetail as fixtureLoopDetail } from '$lib/fixtures/agentLoops';
import type { AgentLoopDetail } from '$lib/types/models';

export interface PageData {
	detail: AgentLoopDetail;
}

async function loadFromApi(id: string, fetch: typeof globalThis.fetch): Promise<PageData> {
	const [resp, messagesResp, variablesResp, graphResp, iterationsResp] =
		await Promise.allSettled([
			client.GET('/api/v1/agent-loops/{id}', {
				fetch,
				params: { path: { id } },
			}),
			client.GET('/api/v1/agent-loops/{id}/messages', {
				fetch,
				params: { path: { id } },
			}),
			client.GET('/api/v1/agent-loops/{id}/variables', {
				fetch,
				params: { path: { id } },
			}),
			client.GET('/api/v1/agent-loops/{id}/graph', {
				fetch,
				params: { path: { id } },
			}),
			client.GET('/api/v1/agent-loops/{id}/iteration-history', {
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

	const baseLoopData = extractData<unknown>(resp);
	const messages = extractData<unknown[]>(messagesResp);
	const variables = extractData<unknown[]>(variablesResp);
	const graph = extractData<unknown>(graphResp);
	const iterations = extractData<unknown[]>(iterationsResp);

	if (!baseLoopData) {
		if (import.meta.env.DEV) {
			console.warn('[api] agent-loops/{id} primary endpoint failed, using fixture detail');
		}
		return { detail: fixtureLoopDetail };
	}

	const detail = adaptAgentLoopDetail(baseLoopData, {
		messages,
		variables,
		graph,
		iterations,
	});
	void adaptAgentLoop; // keep import if we need a summary-only variant later
	return { detail };
}

export async function load({ params, fetch }): Promise<PageData> {
	const id = params.id ?? '';
	if (!id || fallbackEnabled()) {
		return { detail: fixtureLoopDetail };
	}
	try {
		return await loadFromApi(id, fetch as typeof globalThis.fetch);
	} catch (err) {
		if (import.meta.env.DEV) {
			console.warn('[api] agent-loops/{id} load failed, falling back to fixtures', err);
			return { detail: fixtureLoopDetail };
		}
		throw err;
	}
}
