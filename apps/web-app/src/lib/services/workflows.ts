import { client, downloadFile } from '#lib/api/client';
import { call, extractPage, requireData } from '#lib/api/envelope';
import type { PageResult } from '#lib/api/envelope';
import type {
	DefinitionDerivation,
	ExecutionFileScope,
	Workflow,
	WorkflowDefinitionDoc,
	WorkflowDetail,
	WorkflowGraph,
	WorkflowInput,
} from '#lib/types/models';
import { assertFileScope, assertWorkflowInput } from '#lib/services/run';
import { getGraphEdges, getGraphNodes, toWorkflowGraph } from '#lib/services/graph';
import { listDefinitionDrafts, listDefinitionVersions } from '#lib/services/definitions';

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
	edge_count?: number;
	updated_at?: number;
	runs?: number;
	success_rate?: number | null;
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
		derived_from?: {
			source_kind?: string;
			source_id?: string;
			derived_at?: number;
		} | null;
	};
	derived_from?: {
		source_kind?: string;
		source_id?: string;
		derived_at?: number;
	} | null;
}

/** The workflow detail response: the definition document plus the
 * fields the list view reads from it. */
type WorkflowDetailDoc = WorkflowDto & WorkflowDefinitionDoc;

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

function toDerivation(value: unknown): DefinitionDerivation | null {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
	const record = value as Record<string, unknown>;
	const sourceId = typeof record.source_id === 'string' ? record.source_id : '';
	const sourceKind = typeof record.source_kind === 'string' ? record.source_kind : '';
	if (!sourceId || !sourceKind) return null;
	const derivedAt = typeof record.derived_at === 'number' ? record.derived_at : 0;
	return { sourceKind, sourceId, derivedAt };
}

function toWorkflow(d: WorkflowDto): Workflow {
	const metadata = d.metadata ?? {};
	const rawDerivation = d.derived_from ?? metadata.derived_from ?? null;
	return {
		id: d.id ?? '',
		name: d.name ?? d.id ?? '',
		description: d.description ?? '',
		category: d.category ?? '',
		tags: d.tags ?? [],
		author: d.author ?? '',
		version: parseVersion(d.version, 1),
		status: d.status ?? 'active',
		nodeCount: d.node_count ?? 0,
		edgeCount: d.edge_count ?? 0,
		updatedAt: toIso(d.updated_at),
		runs: d.runs ?? 0,
		successRate: d.success_rate ?? null,
		published: d.published ?? metadata.published ?? false,
		derivedFrom: toDerivation(rawDerivation),
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
		await call<WorkflowDetailDoc>(
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
	// The persisted derivation source travels in `metadata.derived_from`;
	// the list projection may omit it, so the definition document wins when
	// present. Absent means sourceless history, never backfilled.
	const fromDefinition = toDerivation(
		(data as Record<string, unknown>).derived_from ??
			((data as Record<string, unknown>).metadata as Record<string, unknown> | undefined)
				?.derived_from ??
			null,
	);
	return {
		...base,
		id,
		graph,
		versions,
		drafts,
		definition: data,
		derivedFrom: fromDefinition ?? base.derivedFrom ?? null,
	};
}

/**
 * Workflow graph from the typed node/edge views. The backend supplies pure
 * topology; layout is computed by the canvas.
 */
export async function getWorkflowGraph(id: string): Promise<WorkflowGraph> {
	const [nodes, edges] = await Promise.all([getGraphNodes(id), getGraphEdges(id)]);
	return toWorkflowGraph(nodes, edges);
}

/** Create a new workflow. */
export async function createWorkflow(name: string, definition: object): Promise<Workflow> {
	const data = requireData(
		await call<WorkflowDto>(client.POST('/api/v1/workflows', { body: { name, definition } })),
		'Workflow creation',
	);
	return toWorkflow(data);
}

/**
 * Clone a workflow into a new id in the same library. The server assigns
 * the new id and records the source workflow as the persisted derivation
 * source, so the clone links back without remark-style lineage.
 */
export async function cloneWorkflow(id: string, newId?: string): Promise<string> {
	const data = await call<string>(
		client.POST('/api/v1/workflows/{id}/clone', {
			params: { path: { id } },
			body: { new_id: newId ?? null },
		}),
	);
	const clonedId = typeof data === 'string' ? data : '';
	if (!clonedId) throw new Error('Clone returned no workflow id');
	return clonedId;
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
export async function replaceWorkflow(id: string, definition: object): Promise<Workflow> {
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
	input?: WorkflowInput | null,
	/**
	 * Optional per-run filesystem scope (`ExecutionFileScope` on the
	 * backend). Absent inherits the global default.
	 */
	fileScope?: ExecutionFileScope | null,
	/**
	 * Pinned workflow version to run. Absent executes the stored
	 * definition as it stands.
	 */
	version?: string | null,
): Promise<string> {
	const checkedInput = assertWorkflowInput(input ?? null);
	const checkedScope = assertFileScope(fileScope ?? null);
	const data = await call<{ execution_id?: string }>(
		client.POST('/api/v1/workflows/{id}/execute', {
			params: { path: { id } },
			body: {
				input: checkedInput,
				file_scope: checkedScope,
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
	const data = await call<string>(client.POST('/api/v1/workflows/import', { body: { json } }));
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
