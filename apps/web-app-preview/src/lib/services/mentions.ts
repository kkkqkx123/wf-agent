import { client } from '#lib/api/client';
import { call } from '#lib/api/envelope';

export interface MentionCandidate {
	id: string;
	kind: string;
	label: string;
	description?: string | null;
	value: string;
	keywords: string[];
	data: Record<string, unknown>;
}

export interface MentionSearchOutcome {
	items: MentionCandidate[];
}

export async function mentionSearch(params: {
	q: string;
	kinds?: string[];
	limit?: number;
}): Promise<MentionSearchOutcome> {
	if (!params.q.trim()) {
		return { items: [] };
	}
	const data = await call<{ items?: MentionCandidate[] }>(
		client.GET('/api/v1/mentions/search', {
			params: {
				query: {
					q: params.q,
					kinds: params.kinds?.join(','),
					limit: params.limit,
				},
			},
		}),
	);
	return { items: data.items ?? [] };
}
