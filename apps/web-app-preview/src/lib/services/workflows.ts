import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { Workflow, WorkflowDetail, WorkflowGraph, WorkflowVersion } from '$lib/types/models';

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

function toGraph(d: { nodes?: GraphNodeDto[]; edges?: GraphEdgeDto[] } | undefined): WorkflowGraph {
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
    // graph, versions, drafts, neighbors come from separate endpoints
    const [graphRes, versionsRes] = await Promise.allSettled([
        call<unknown>(
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            client.GET('/api/v1/workflows/{id}/graph' as any, {
                // eslint-disable-next-line @typescript-eslint/no-explicit-any
                params: { path: { id } } as any,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
            } as any),
        ),
        call<unknown>(
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            client.GET('/api/v1/workflows/{id}/versions' as any, {
                // eslint-disable-next-line @typescript-eslint/no-explicit-any
                params: { path: { id } } as any,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
            } as any),
        ),
    ]);

    const graph = graphRes.status === 'fulfilled' ? toGraph(graphRes.value as { nodes?: GraphNodeDto[]; edges?: GraphEdgeDto[] }) : { nodes: [], edges: [] };
    const versions = versionsRes.status === 'fulfilled'
        ? (Array.isArray(versionsRes.value) ? versionsRes.value : (versionsRes.value as { items?: VersionDto[] })?.items ?? [])
            .map((v) => toVersion(v as VersionDto))
        : [];

    return {
        ...wf,
        graph,
        versions,
        drafts: [],
        neighbors: [],
    };
}
