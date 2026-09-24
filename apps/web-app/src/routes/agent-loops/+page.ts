/**
 * Agent-loops list route loader.
 *
 *  - Real API: GET /api/v1/agent-loops/summaries → PageView.items → adaptAgentLoop
 *  - Fallback: `VITE_API_FALLBACK=true` or request failure → fixtures
 *
 * Envelope peeling lives in `unwrapPage`; schema types come from `schema.d.ts`.
 */
import { client } from '$lib/api/client';
import { unwrapPage } from '$lib/api/envelope';
import { fallbackEnabled } from '$lib/api/fallback';
import { adaptAgentLoop } from '$lib/api/adapters/agent';
import { agentLoops as fixtureAgentLoops, loopDetail as fixtureLoopDetail } from '$lib/fixtures/agentLoops';
import type { AgentLoop, AgentLoopDetail } from '$lib/types/models';

export interface PageData {
	agentLoops: AgentLoop[];
	loopDetail: AgentLoopDetail;
}

async function loadFromApi(fetch: typeof globalThis.fetch): Promise<PageData> {
	const [listResp, statsResp] = await Promise.all([
		client.GET('/api/v1/agent-loops/summaries', { fetch }),
		client.GET('/api/v1/agent-loops/stats', { fetch }).catch(() => null),
	]);
	const page = unwrapPage<unknown>(listResp);
	const agentLoops = page.items.map(adaptAgentLoop);

	// Inspector pane needs a detail view.  `/summaries` returns list rows
	// only — fetch the first loop's full detail for the inspector, or
	// fall back to the fixture detail shape if the API is empty.
	let loopDetail: AgentLoopDetail = fixtureLoopDetail;
	const firstId = agentLoops[0]?.id;
	if (firstId) {
		try {
			const detailResp = await client.GET('/api/v1/agent-loops/{id}', {
				fetch,
				params: { path: { id: firstId } },
			});
			const raw = (detailResp.data as { success?: boolean; data?: unknown } | undefined)?.data;
			if (raw) {
				const { adaptAgentLoopDetail } = await import('$lib/api/adapters/agent');
				loopDetail = adaptAgentLoopDetail(raw) as AgentLoopDetail;
			}
		} catch {
			// keep fixture detail shape — inspector renders an empty-state variant
		}
	}
	// statsResp intentionally unused right now; page renders a single metric grid later.
	void statsResp;

	return { agentLoops, loopDetail };
}

export async function load({ fetch }): Promise<PageData> {
	if (fallbackEnabled()) {
		return { agentLoops: fixtureAgentLoops, loopDetail: fixtureLoopDetail };
	}
	try {
		return await loadFromApi(fetch as typeof globalThis.fetch);
	} catch (err) {
		if (import.meta.env.DEV) {
			console.warn('[api] agent-loops load failed, falling back to fixtures', err);
			return { agentLoops: fixtureAgentLoops, loopDetail: fixtureLoopDetail };
		}
		throw err;
	}
}
