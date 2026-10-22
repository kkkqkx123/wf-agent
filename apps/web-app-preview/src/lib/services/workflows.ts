import { client, downloadFile } from '#lib/api/client';
import { call, extractPage, requireData } from '#lib/api/envelope';
import type { PageResult } from '#lib/api/envelope';
import type {
	Workflow,
	WorkflowDetail,
	WorkflowGraph,
} from '#lib/types/models';
import {
	getGraphEdges,
	getGraphNodes,
	toWorkflowGraph,
} from '#lib/services/graph';
import {
	listDefinitionDrafts,
	listDefinitionVersions,
} from '#lib/services/definitions';

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
	/** Whether the workflow is published to the template library. */
	published?: boolean;
	/** The workflow definition's own metadata. */
	metadata?: {
		author?: string;
		tags?: string[];
		category?: string;
		published?: boolean;
		is_public?: boolean;
		is_system?: boolean;
		enabled?: boolean;
	};
}

function parseVersion(
	value: string | number | undefined,
	fallback: number,
): number {
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
		successRate: d.success_rate ?? d.successRate ?? null,
		published: d.published ?? d.metadata?.published ?? false,
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
			params: {
				query: {
					limit: params?.limit,
					offset: params?.offset,
					name: params?.name,
				},
			},
		}),
	);
	requireData(data, 'Workflow list');
	const page = extractPage<WorkflowDto>(data);
	return { ...page, items: page.items.map((d) => toWorkflow(d)) };
}

/** Detailed view of a single workflow with true aggregation. */
export async function getWorkflowDetail(id: string): Promise<WorkflowDetail> {
	const data = requireData(
		await call<WorkflowDto>(
			client.GET('/api/v1/workflows/{id}', { params: { path: { id } } }),
		),
		`Workflow ${id}`,
	);
	const base = toWorkflow(data);
	const [graph, versions, drafts] = await Promise.all([
		getWorkflowGraph(id),
		listDefinitionVersions('workflow', id),
		listDefinitionDrafts('workflow', id),
	]);
	return { ...base, id, graph, versions, drafts, metadata: data.metadata };
}

/** Compatibility alias. */
export async function getWorkflow(id: string): Promise<WorkflowDetail> {
	return getWorkflowDetail(id);
}

/**
 * Workflow graph from the typed node/edge views. The backend supplies pure
 * topology; layout is computed by the canvas.
 */
export async function getWorkflowGraph(id: string): Promise<WorkflowGraph> {
	const [nodes, edges] = await Promise.all([
		getGraphNodes(id),
		getGraphEdges(id),
	]);
	return toWorkflowGraph(nodes, edges);
}

interface DefinitionDto {
	id?: unknown;
	name?: unknown;
	version?: unknown;
	created_at?: unknown;
	updated_at?: unknown;
	metadata?: { author?: unknown } | null;
	nodes?: unknown;
	edges?: unknown;
}

/** Create a new workflow. */
export async function createWorkflow(
	name: string,
	definition: object,
): Promise<Workflow> {
	const data = requireData(
		await call<WorkflowDto>(
			client.POST('/api/v1/workflows', { body: { name, definition } }),
		),
		'Workflow creation',
	);
	return toWorkflow(data);
}

/** Partial update goes through the metadata sub-resource, never the item route. */
export async function updateWorkflow(
	id: string,
	updates: Partial<Workflow>,
): Promise<Workflow> {
	const metadata: Record<string, unknown> = {};
	if (updates.name !== undefined) metadata.name = updates.name;
	if (updates.description !== undefined)
		metadata.description = updates.description;
	if (updates.category !== undefined) metadata.category = updates.category;
	if (updates.tags !== undefined) metadata.tags = updates.tags;
	if (updates.author !== undefined) metadata.author = updates.author;
	if (updates.status !== undefined) metadata.status = updates.status;
	await call<unknown>(
		client.PATCH('/api/v1/workflows/{id}/metadata', {
			params: { path: { id } },
			body: metadata,
		}),
	);
	const detail = await getWorkflowDetail(id).catch(() => null);
	return detail ?? { ...toWorkflow({ id }), ...updates, id };
}

/**
 * Full-document replace goes through the item route with a complete
 * definition. The backend overwrites the stored document and echoes the id,
 * so callers must pass nodes, edges and config — never a partial patch.
 */
export async function replaceWorkflow(
	id: string,
	definition: object,
): Promise<Workflow> {
	const savedId = await call<string>(
		client.PUT('/api/v1/workflows/{id}', {
			params: { path: { id } },
			body: definition,
		}),
	);
	return toWorkflow({ id: savedId ?? id });
}

/** Delete a workflow. */
export async function deleteWorkflow(id: string): Promise<boolean> {
	const { response } = await client.DELETE('/api/v1/workflows/{id}', {
		params: { path: { id } },
	});
	return response.ok;
}

/** Execute a workflow, returning the new execution id. */
export async function executeWorkflow(
	id: string,
	input?: unknown,
	/**
	 * Optional per-run filesystem scope (`ExecutionFileScope` on the
	 * backend). Absent inherits the global default.
	 */
	fileScope?: unknown,
	/**
	 * Pinned workflow version to run. Absent executes the stored
	 * definition as it stands.
	 */
	version?: string | null,
): Promise<string> {
	const data = await call<{ execution_id?: string }>(
		client.POST('/api/v1/workflows/{id}/execute', {
			params: { path: { id } },
			body: {
				input: input ?? null,
				file_scope: fileScope ?? null,
				version: version ?? null,
			},
		}),
	);
	const executionId = data?.execution_id ?? '';
	if (!executionId) throw new Error('Execute returned no execution id');
	return executionId;
}

/** Download the workflow definition as a JSON file. */
export async function exportWorkflow(id: string): Promise<void> {
	await downloadFile(
		`/api/v1/workflows/${encodeURIComponent(id)}/export?download=true`,
		`workflow-${id}.json`,
	);
}

/** Import a workflow definition from JSON text, returning the new id. */
export async function importWorkflow(json: string): Promise<string> {
	const data = await call<string>(
		client.POST('/api/v1/workflows/import', { body: { json } }),
	);
	if (!data) throw new Error('Import returned no id');
	return data;
}

/** Create a minimal start→end workflow and return it. */
export async function createMinimalWorkflow(name: string): Promise<Workflow> {
	const now = Date.now();
	const trimmed = name.trim() || `Workflow ${now}`;
	const suffix =
		typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
			? crypto.randomUUID().slice(0, 8)
			: Math.random().toString(36).slice(2, 10);
	const id = `wf-${suffix}`;
	return createWorkflow(trimmed, {
		id,
		name: trimmed,
		version: '1.0.0',
		nodes: [
			{ id: 'start', node_type: 'START', name: 'start' },
			{ id: 'end', node_type: 'END', name: 'end' },
		],
		edges: [
			{
				id: 'e1',
				source_node_id: 'start',
				target_node_id: 'end',
				type: 'DEFAULT',
			},
		],
		created_at: now,
		updated_at: now,
	});
}
