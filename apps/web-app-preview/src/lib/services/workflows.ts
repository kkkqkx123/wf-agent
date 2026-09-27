import { client, downloadFile } from '$lib/api/client';
import { call, extractPage, requireData } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	Workflow,
	WorkflowDetail,
	WorkflowDraft,
	WorkflowGraph,
	WorkflowVersion,
} from '$lib/types/models';
import {
	getGraphEdges,
	getGraphNodes,
	toWorkflowGraph,
	validateWorkflowDraft,
} from '$lib/services/graph';

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
		getWorkflowVersions(id),
		listWorkflowDrafts(),
	]);
	return { ...base, id, graph, versions, drafts };
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

function definitionVersion(value: unknown, fallback: string): string {
	return typeof value === 'string' && value ? value : fallback;
}

/** Workflow version history: full definitions narrowed to row fields. */
export async function getWorkflowVersions(
	id: string,
): Promise<WorkflowVersion[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/workflows/{id}/versions', { params: { path: { id } } }),
	);
	requireData(data, `Version list missing for workflow ${id}`);
	const rows = Array.isArray(data) ? (data as DefinitionDto[]) : [];
	return rows.map((d, index) => ({
		version: definitionVersion(d.version, `v${index + 1}`),
		createdAt: toIso(
			(typeof d.created_at === 'number' ? d.created_at : null) ??
				(typeof d.updated_at === 'number' ? d.updated_at : null),
		),
		author:
			typeof d.metadata?.author === 'string' ? d.metadata.author : '',
		note: typeof d.name === 'string' ? d.name : '',
		current: index === 0,
	}));
}

/** One saved version definition (free-form full definition). */
export async function getWorkflowVersionDefinition(
	id: string,
	version: string,
): Promise<DefinitionDto> {
	const data = await call<unknown>(
		client.GET('/api/v1/workflows/{id}/versions/{version}', {
			params: { path: { id, version } },
		}),
	);
	return requireData(data, `Version ${version} missing for workflow ${id}`) as DefinitionDto;
}

interface DiffNodeDto {
	id?: unknown;
}

interface DiffEdgeDto {
	id?: unknown;
	source_node_id?: unknown;
	target_node_id?: unknown;
}

function edgeKey(edge: DiffEdgeDto): string {
	return `${String(edge.source_node_id ?? '')}->${String(edge.target_node_id ?? '')}`;
}

export interface VersionDiff {
	addedNodes: string[];
	removedNodes: string[];
	addedEdges: string[];
	removedEdges: string[];
}

/** Structural diff between two saved versions, computed client-side. */
export async function diffWorkflowVersions(
	id: string,
	from: string,
	to: string,
): Promise<VersionDiff> {
	const [a, b] = await Promise.all([
		getWorkflowVersionDefinition(id, from),
		getWorkflowVersionDefinition(id, to),
	]);
	const nodeIds = (d: DefinitionDto): Set<string> =>
		new Set(
			(Array.isArray(d.nodes) ? (d.nodes as DiffNodeDto[]) : [])
				.map((node) => String(node.id ?? ''))
				.filter(Boolean),
		);
	const edgeKeys = (d: DefinitionDto): Set<string> =>
		new Set(
			(Array.isArray(d.edges) ? (d.edges as DiffEdgeDto[]) : []).map(edgeKey),
		);
	const before = nodeIds(a);
	const after = nodeIds(b);
	const beforeEdges = edgeKeys(a);
	const afterEdges = edgeKeys(b);
	return {
		addedNodes: [...after].filter((node) => !before.has(node)),
		removedNodes: [...before].filter((node) => !after.has(node)),
		addedEdges: [...afterEdges].filter((edge) => !beforeEdges.has(edge)),
		removedEdges: [...beforeEdges].filter((edge) => !afterEdges.has(edge)),
	};
}

/**
 * Editable drafts with validation attached. Drafts may be incomplete, so
 * each entry carries its current validation issues.
 */
export async function listWorkflowDrafts(): Promise<WorkflowDraft[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/workflows/drafts'),
	);
	requireData(data, 'Draft list');
	const rows = Array.isArray(data) ? (data as DefinitionDto[]) : [];
	const validated = await Promise.allSettled(
		rows.map((row) =>
			validateWorkflowDraft(String(row.id ?? '')).then((issues) => ({
				id: String(row.id ?? ''),
				name:
					typeof row.name === 'string' ? row.name : String(row.id ?? ''),
				updatedAt: toIso(
					(typeof row.updated_at === 'number' ? row.updated_at : null) ??
						(typeof row.created_at === 'number' ? row.created_at : null),
				),
				valid: issues.length === 0,
				issues: issues.map((issue) => `${issue.field}: ${issue.message}`),
			})),
		),
	);
	return validated
		.filter(
			(result): result is PromiseFulfilledResult<WorkflowDraft> =>
				result.status === 'fulfilled',
		)
		.map((result) => result.value);
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
): Promise<string> {
	const data = await call<{ execution_id?: string }>(
		client.POST('/api/v1/workflows/{id}/execute', {
			params: { path: { id } },
			body: { input: input ?? null },
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
	const id = `wf-${now}`;
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
