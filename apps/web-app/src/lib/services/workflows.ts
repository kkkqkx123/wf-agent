import { client, request, downloadFile } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	Workflow,
	WorkflowDetail,
	WorkflowDraft,
	WorkflowGraph,
	WorkflowVersion,
} from '$lib/types/models';

interface WorkflowDto {
	id?: string;
	name?: string;
	description?: string;
	category?: string;
	tags?: string[];
	author?: string;
	version?: number;
	status?: string;
	node_count?: number;
	edge_count?: number;
	updated_at?: string;
	runs?: number;
	run_count?: number;
	success_rate?: number | null;
}

interface GraphNodeDto {
	id?: string;
	label?: string;
	kind?: string;
	status?: string;
	x?: number;
	y?: number;
}

interface GraphEdgeDto {
	id?: string;
	from?: string;
	to?: string;
	label?: string;
}

interface VersionDto {
	version?: number;
	created_at?: string;
	author?: string;
	note?: string;
	current?: boolean;
}

function toWorkflow(d: WorkflowDto): Workflow {
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		description: d.description ?? '',
		category: d.category ?? '',
		tags: d.tags ?? [],
		author: d.author ?? '',
		version: d.version ?? 1,
		status: d.status ?? 'active',
		nodeCount: d.node_count ?? 0,
		edgeCount: d.edge_count ?? 0,
		updatedAt: d.updated_at ?? '',
		runs: d.runs ?? d.run_count ?? 0,
		successRate: d.success_rate ?? null,
	};
}

function toGraph(
	d: { nodes?: GraphNodeDto[]; edges?: GraphEdgeDto[] } | undefined,
): WorkflowGraph {
	return {
		nodes: (d?.nodes ?? []).map((n) => ({
			id: n.id ?? '',
			label: n.label ?? n.id ?? '',
			kind: n.kind ?? 'task',
			status: n.status,
			x: n.x ?? 0,
			y: n.y ?? 0,
		})),
		edges: (d?.edges ?? []).map((e) => ({
			id: e.id ?? `${e.from ?? ''}-${e.to ?? ''}`,
			from: e.from ?? '',
			to: e.to ?? '',
			label: e.label,
		})),
	};
}

function toVersion(d: VersionDto): WorkflowVersion {
	return {
		version: d.version ?? 1,
		createdAt: d.created_at ?? '',
		author: d.author ?? '',
		note: d.note ?? '',
		current: d.current ?? false,
	};
}

export async function listWorkflows(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Workflow>> {
	const data = await call<unknown>(
		client.GET('/api/v1/workflows', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<WorkflowDto>(data);
	return { ...page, items: page.items.map(toWorkflow) };
}

export async function getWorkflow(id: string): Promise<WorkflowDetail> {
	const data = await call<WorkflowDto>(
		client.GET('/api/v1/workflows/{id}', {
			params: { path: { id } },
		}),
	);
	const wf = toWorkflow(data);
	// graph and versions are served by their own endpoints
	const [graphRes, versionsRes] = await Promise.allSettled([
		call<unknown>(
			client.GET('/api/v1/workflows/{id}/graph', {
				params: { path: { id } },
			}),
		),
		call<unknown>(
			client.GET('/api/v1/workflows/{id}/versions', {
				params: { path: { id } },
			}),
		),
	]);

	const graph =
		graphRes.status === 'fulfilled'
			? toGraph(
					graphRes.value as { nodes?: GraphNodeDto[]; edges?: GraphEdgeDto[] },
				)
			: { nodes: [], edges: [] };
	const versions =
		versionsRes.status === 'fulfilled'
			? (Array.isArray(versionsRes.value)
					? versionsRes.value
					: ((versionsRes.value as { items?: VersionDto[] })?.items ?? [])
				).map((v) => toVersion(v as VersionDto))
			: [];

	return { ...wf, graph, versions };
}

interface DraftDto {
	id?: string;
	name?: string;
	updated_at?: string;
}

interface ValidationIssueDto {
	field?: string;
	message?: string;
}

/**
 * Every stored draft, across all workflows. Drafts are persisted as whole
 * definitions under the workflow id they edit, so `id` doubles as the workflow
 * id and the caller filters to the workflow in view.
 */
export async function listWorkflowDrafts(): Promise<WorkflowDraft[]> {
	// `handle_list_drafts` is one of the duplicated utoipa handler names.
	const data = await call<unknown>(request('GET', '/api/v1/workflows/drafts'));
	return (Array.isArray(data) ? (data as DraftDto[]) : []).map((d) => ({
		id: d.id ?? '',
		name: d.name ?? d.id ?? '',
		updatedAt: d.updated_at ?? '',
	}));
}

/** Publish-blocking issues for one draft; an empty list means it can promote. */
export async function validateWorkflowDraft(id: string): Promise<string[]> {
	const data = await call<unknown>(
		request('GET', '/api/v1/workflows/drafts/{id}/validate', {
			params: { path: { id } },
		}),
	);
	const issues = Array.isArray(data) ? (data as ValidationIssueDto[]) : [];
	return issues.map((issue) =>
		issue.field
			? `${issue.field}: ${issue.message ?? ''}`
			: (issue.message ?? ''),
	);
}

export async function promoteWorkflowDraft(id: string): Promise<void> {
	await call<unknown>(
		request('POST', '/api/v1/workflows/drafts/{id}/promote', {
			params: { path: { id } },
		}),
	);
}

/** Re-point the formal definition at a stored version. */
export async function rollbackWorkflow(
	id: string,
	version: number,
): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/workflows/{id}/rollback', {
			params: { path: { id } },
			body: { version: String(version) },
		}),
	);
}

/** Ask the server to render the definition and save it as a download. */
export async function exportWorkflow(id: string): Promise<void> {
	await downloadFile(
		`/api/v1/workflows/${encodeURIComponent(id)}/export?download=true`,
		`workflow-${id}.json`,
	);
}

/** Import a workflow from the JSON text of a full definition; returns its id. */
export async function importWorkflow(json: string): Promise<string> {
	const data = await call<unknown>(
		client.POST('/api/v1/workflows/import', {
			body: { json },
		}),
	);
	return typeof data === 'string' ? data : '';
}
