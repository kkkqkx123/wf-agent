import { client } from '$lib/api/client';
import { call } from '$lib/api/envelope';

export interface SearchHit {
	id: string;
	type: string;
	label: string;
	matches: string[];
	executionId: string | null;
	agentLoopId: string | null;
}

export interface SearchOutcome {
	query: string;
	items: SearchHit[];
	total: number;
	truncated: boolean;
	nextCursor: string | null;
}

interface SearchHitDto {
	id?: string;
	type?: string;
	label?: string;
	matches?: string[];
	execution_id?: string | null;
	agent_loop_id?: string | null;
}

interface SearchDto {
	query?: string;
	items?: SearchHitDto[];
	total?: number;
	truncated?: boolean;
	next_cursor?: string | null;
}

function toHit(d: SearchHitDto): SearchHit {
	return {
		id: d.id ?? '',
		type: d.type ?? '',
		label: d.label ?? d.id ?? '',
		matches: d.matches ?? [],
		executionId: d.execution_id ?? null,
		agentLoopId: d.agent_loop_id ?? null,
	};
}

/**
 * Run a cross-source search. Empty queries are rejected by the server, so
 * blank input short-circuits to an empty outcome without a request.
 */
export async function unifiedSearch(params: {
	q: string;
	types?: string;
	limit?: number;
	cursor?: string;
}): Promise<SearchOutcome> {
	if (!params.q.trim()) {
		return {
			query: params.q,
			items: [],
			total: 0,
			truncated: false,
			nextCursor: null,
		};
	}
	const data = await call<SearchDto>(
		client.GET('/api/v1/search', {
			params: {
				query: {
					q: params.q,
					types: params.types,
					limit: params.limit,
					cursor: params.cursor,
				},
			},
		}),
	);
	return {
		query: data.query ?? params.q,
		items: (data.items ?? []).map(toHit),
		total: data.total ?? 0,
		truncated: data.truncated ?? false,
		nextCursor: data.next_cursor ?? null,
	};
}
