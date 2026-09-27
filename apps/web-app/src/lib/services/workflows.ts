import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { Workflow, WorkflowDetail, WorkflowGraph, WorkflowVersion } from '$lib/types/models';

interface WorkflowDto {
	id?: string;
	name?: string;
	description?: string | null;
	category?: string;
	tags?: string[];
	author?: string;
	version?: string | number;
	status?: string;
	node_count?: number;
	nodeCount?: number;
	edge_count?: number;
	edgeCount?: number;
	updated_at?: number;
	updatedAt?: string;
	runs?: number;
	success_rate?: number | null;
	successRate?: number | null;
}

function parseVersion(value: string | number | undefined, fallback: number): number {
	if (typeof value === 'number') return value;
	if (typeof value === 'string') {
		const parsed = parseInt(value, 10);
		return Number.isNaN(parsed) ? fallback : parsed;
	}
	return fallback;
}

function toIso(value: number | string | null | undefined): string {
	if (typeof value === 'string') return value;
	if (typeof value === 'number') return new Date(value).toISOString();
	return '';
}

function toWorkflow(d: WorkflowDto): Workflow {
	return {
		id: d.id ?? '',
		name: d.name ?? d.id ?? '',
		description: d.description ?? '',
		category: d.category ?? '',
		tags: d.tags ?? [],
		author: d.author ?? '',
		version: parseVersion(d.version, 1),
		status: d.status ?? 'active',
		nodeCount: d.node_count ?? d.nodeCount ?? 0,
		edgeCount: d.edge_count ?? d.edgeCount ?? 0,
		updatedAt: toIso(d.updated_at ?? d.updatedAt),
		runs: d.runs ?? 0,
		successRate: d.success_rate ?? d.successRate ?? null
	};
}

/** List workflows with optional paging. */
export async function listWorkflows(params?: {
	limit?: number;
	offset?: number;
	name?: string;
}): Promise<PageResult<Workflow>> {
	const data = await call<unknown>(
		client.GET('/api/v1/workflows', {
			params: { query: { limit: params?.limit, offset: params?.offset, name: params?.name } }
		})
	);
	const page = extractPage<WorkflowDto>(data);
	return { ...page, items: page.items.map((d) => toWorkflow(d)) };
}

/** Detailed view of a single workflow. */
export async function getWorkflowDetail(id: string): Promise<WorkflowDetail> {
	const data = await call<WorkflowDto>(
		client.GET('/api/v1/workflows/{id}', { params: { path: { id } } })
	);
	const base = toWorkflow(data ?? {});
	const [graph, versions] = await Promise.all([
		getWorkflowGraph(id).catch(() => ({ nodes: [], edges: [] }) as WorkflowGraph),
		getWorkflowVersions(id).catch(() => [] as WorkflowVersion[])
	]);
	return { ...base, id, graph, versions, drafts: [], neighbors: [] };
}

/** Compatibility alias. */
export async function getWorkflow(id: string): Promise<WorkflowDetail> {
	return getWorkflowDetail(id);
}

/** Workflow graph visualization data. */
export async function getWorkflowGraph(id: string): Promise<WorkflowGraph> {
	const data = await call<WorkflowGraph>(
		client.GET('/api/v1/workflows/{id}/graph', { params: { path: { id } } })
	);
	if (!data || !Array.isArray(data.nodes)) return { nodes: [], edges: [] };
	return data;
}

interface VersionDto {
	version?: number;
	created_at?: number;
	createdAt?: string;
	author?: string;
	note?: string;
	message?: string;
	current?: boolean;
}

/** Workflow version history. */
export async function getWorkflowVersions(id: string): Promise<WorkflowVersion[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/workflows/{id}/versions', { params: { path: { id } } })
	);
	const page = extractPage<VersionDto>(data);
	const rows = page.items.length > 0 ? page.items : Array.isArray(data) ? (data as VersionDto[]) : [];
	return rows.map((d, index) => ({
		version: d.version ?? index + 1,
		createdAt:
			typeof d.createdAt === 'string'
				? d.createdAt
				: d.created_at
					? new Date(d.created_at).toISOString()
					: '',
		author: d.author ?? '',
		note: d.note ?? d.message ?? '',
		current: d.current ?? index === 0
	}));
}

/** Create a new workflow. */
export async function createWorkflow(name: string, definition: object): Promise<Workflow> {
	const data = await call<WorkflowDto>(
		client.POST('/api/v1/workflows', { body: { name, definition } })
	);
	return toWorkflow(data ?? {});
}

/** Partial update goes through the metadata sub-resource, never the item route. */
export async function updateWorkflow(id: string, updates: Partial<Workflow>): Promise<Workflow> {
	const metadata: Record<string, unknown> = {};
	if (updates.name !== undefined) metadata.name = updates.name;
	if (updates.description !== undefined) metadata.description = updates.description;
	if (updates.category !== undefined) metadata.category = updates.category;
	if (updates.tags !== undefined) metadata.tags = updates.tags;
	if (updates.author !== undefined) metadata.author = updates.author;
	if (updates.status !== undefined) metadata.status = updates.status;
	await call<unknown>(
		client.PATCH('/api/v1/workflows/{id}/metadata', {
			params: { path: { id } },
			body: metadata
		})
	);
	const detail = await getWorkflowDetail(id).catch(() => null);
	return detail ?? { ...toWorkflow({ id }), ...updates, id };
}

/**
 * Full-document replace goes through the item route with a complete
 * definition. The backend overwrites the stored document and echoes the id,
 * so callers must pass nodes, edges and config — never a partial patch.
 */
export async function replaceWorkflow(id: string, definition: object): Promise<Workflow> {
	const savedId = await call<string>(
		client.PUT('/api/v1/workflows/{id}', {
			params: { path: { id } },
			body: definition
		})
	);
	return toWorkflow({ id: savedId ?? id });
}

/** Delete a workflow. */
export async function deleteWorkflow(id: string): Promise<boolean> {
	const { response } = await client.DELETE('/api/v1/workflows/{id}', {
		params: { path: { id } }
	});
	return response.ok;
}
