import { client } from '$lib/api/client';
import { call, extractPage, requireData } from '$lib/api/envelope';

/**
 * One node template as returned by `GET /api/v1/templates/node`. The backend
 * summary carries exactly `id`, `name`, `node_type`, an optional
 * `description` and `updated_at`; nothing else is available to browse by.
 *
 * `nodeType` is whatever the template stored: a builtin kind, a
 * plugin-contributed name, or (for malformed rows) blank. Normalise before
 * using it as a node kind.
 */
export interface NodeTemplateSummary {
	id: string;
	name: string;
	nodeType: string;
	description: string;
}

export interface NodeTemplatePage {
	items: NodeTemplateSummary[];
	/** True when the backend held back more rows behind `limit`. */
	hasMore: boolean;
	/** Rows dropped because they did not match the summary shape. */
	skipped: number;
}

/**
 * Project one backend row onto the browse model. Rows that do not carry the
 * required fields are dropped rather than patched with defaults, so a
 * backend change surfaces as missing entries instead of silently wrong ones.
 */
function toSummary(row: unknown): NodeTemplateSummary | null {
	if (row === null || typeof row !== 'object') return null;
	const record = row as Record<string, unknown>;
	const { id, name, node_type } = record;
	if (typeof id !== 'string') return null;
	if (typeof name !== 'string') return null;
	if (typeof node_type !== 'string') return null;
	return {
		id,
		name,
		nodeType: node_type,
		description:
			typeof record.description === 'string' ? record.description : '',
	};
}

/**
 * Browse the node template library for the add-node drawer. The endpoint is
 * paginated; a generous limit keeps the drawer's local filter meaningful.
 */
export async function listNodeTemplates(params?: {
	limit?: number;
}): Promise<NodeTemplatePage> {
	const limit = params?.limit ?? 200;
	const data = await call<unknown>(
		client.GET('/api/v1/templates/node', {
			params: { query: { limit } },
		}),
	);
	requireData(data, 'Node templates');
	const page = extractPage<unknown>(data);
	const items: NodeTemplateSummary[] = [];
	let skipped = 0;
	for (const row of page.items) {
		const summary = toSummary(row);
		if (summary) items.push(summary);
		else skipped += 1;
	}
	return { items, hasMore: page.hasMore, skipped };
}
