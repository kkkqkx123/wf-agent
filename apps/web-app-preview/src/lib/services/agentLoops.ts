import { client } from '$lib/api/client';
import { call } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	AgentLoop,
	AgentLoopDetail,
	LoopMessage,
	LoopVariable,
	WorkflowGraph,
} from '$lib/types/models';
import {
	listAgentLoops as listApiLoops,
	getAgentLoop as getApiLoop,
	listLoopMessages as listApiMessages,
	listLoopVariables as listApiVariables,
	getLoopGraph as getApiGraph,
	cancelAgentLoop as cancelApiLoop,
} from '$lib/services/agent-loops';

/** List agent loops with optional paging. */
export async function listAgentLoops(params?: {
	limit?: number;
	offset?: number;
	status?: string;
}): Promise<PageResult<AgentLoop>> {
	const page = await listApiLoops({
		limit: params?.limit,
		offset: params?.offset,
		status: params?.status,
	});
	return {
		...page,
		items: page.items.map(
			(row) =>
				({
					id: row.id,
					name: row.name ?? row.id,
					status: row.status,
					iteration: row.iteration,
					maxIterations: row.maxIterations ?? row.iteration,
					model: row.model ?? '',
					tokens: row.tokens ?? 0,
					startedAt: row.startedAt,
					updatedAt: row.updatedAt ?? row.startedAt,
					checkpoints: row.checkpoints ?? 0,
					errors: row.errors ?? 0,
					starred: row.starred ?? false,
					tags: row.tags ?? [],
				}) satisfies AgentLoop,
		),
	};
}

/** Detailed view of a single agent loop. */
export async function getAgentLoopDetail(id: string): Promise<AgentLoopDetail> {
	const [summary, messages, variables, graph] = await Promise.all([
		getApiLoop(id).catch(() => null),
		listApiMessages(id).catch(() => null),
		listApiVariables(id).catch(() => null),
		getApiGraph(id).catch(() => null),
	]);
	return {
		id,
		name: summary?.name ?? id,
		status: summary?.status ?? '',
		iteration: summary?.iteration ?? 0,
		maxIterations: summary?.maxIterations ?? summary?.iteration ?? 0,
		model: summary?.model ?? '',
		tokens: summary?.tokens ?? 0,
		startedAt: summary?.startedAt ?? '',
		updatedAt: summary?.updatedAt ?? summary?.startedAt ?? '',
		checkpoints: summary?.checkpoints ?? 0,
		errors: summary?.errors ?? 0,
		starred: summary?.starred ?? false,
		tags: summary?.tags ?? [],
		summary: '',
		messages: messages?.items ?? [],
		variables: variables ?? [],
		iterations: [],
		graph: graph ?? { nodes: [], edges: [] },
		analysis: {
			rootCause: null,
			errorChain: [],
			recoveryHints: [],
			toolFrequency: [],
		},
	};
}

/** Conversation messages for an agent loop. */
export async function getAgentLoopMessages(
	loopId: string,
): Promise<LoopMessage[]> {
	const page = await listApiMessages(loopId);
	return page.items;
}

/** Variables snapshot for an agent loop. */
export async function getAgentLoopVariables(
	loopId: string,
): Promise<LoopVariable[]> {
	return listApiVariables(loopId);
}

/** Execution graph for an agent loop. */
export async function getAgentLoopGraph(
	loopId: string,
): Promise<WorkflowGraph> {
	return getApiGraph(loopId);
}

/** Cancel a running agent loop. */
export async function cancelAgentLoop(id: string): Promise<boolean> {
	await cancelApiLoop(id);
	return true;
}

/** Statistics for agent loops. */
export async function getAgentLoopStats(): Promise<object> {
	const data = await call<object>(client.GET('/api/v1/agent-loops/stats'));
	return data ?? {};
}

/** Clean up completed agent loops. */
export async function cleanupCompletedLoops(
	cutoffDays?: number,
): Promise<number> {
	const { request } = await import('$lib/api/client');
	const data = await call<{ deletedCount?: number }>(
		request('POST', '/api/v1/agent-loops/cleanup-completed', {
			body: { cutoffDays },
		}),
	);
	return data?.deletedCount ?? 0;
}
