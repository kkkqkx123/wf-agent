import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	Execution,
	ExecutionDetail,
	KeyValue,
	ToolCallEntry,
	TimelineEntry,
} from '$lib/types/models';

/** Stored execution: both endpoints answer with this shape. */
interface ExecutionDto {
	id?: string;
	workflow_id?: string;
	status?: string;
	current_node_id?: string | null;
	started_at?: number;
	completed_at?: number | null;
	error?: string | null;
	errors?: string[] | null;
	execution_type?: string | null;
	variables?: VariableDto[] | null;
	input?: unknown;
	output?: unknown;
	node_results?: NodeResultDto[] | null;
	graph?: { nodes?: unknown[] } | null;
}

interface NodeResultDto {
	status?: string;
}

interface VariableDto {
	name?: string;
	value?: unknown;
}

function toIso(value: number | null | undefined): string {
	return value === null || value === undefined
		? ''
		: new Date(value).toISOString();
}

/** JSON text for a payload segment the server keeps as an arbitrary value. */
function asJsonText(value: unknown): string | null {
	if (value === null || value === undefined) return null;
	return JSON.stringify(value, null, 2);
}

function toVariable(d: VariableDto): KeyValue {
	return {
		key: d.name ?? '',
		value: typeof d.value === 'string' ? d.value : JSON.stringify(d.value),
	};
}

function toExecution(d: ExecutionDto): Execution {
	const results = d.node_results ?? null;
	const nodesDone = results === null ? null : results.length;
	// The recorded node count is only a total once the run is over; progress
	// needs the graph to know what remains, so it stays undecidable without it.
	const nodesTotal = d.graph?.nodes ? d.graph.nodes.length : null;
	const nodesFailed =
		results === null
			? null
			: results.filter((node) => node.status === 'failed').length;
	const startedAt = d.started_at ?? null;
	const completedAt = d.completed_at ?? null;
	return {
		id: d.id ?? '',
		workflowId: d.workflow_id ?? '',
		status: d.status ?? '',
		currentNodeId: d.current_node_id ?? null,
		startedAt: toIso(startedAt),
		endedAt: completedAt === null ? null : toIso(completedAt),
		nodesDone,
		nodesTotal,
		nodesFailed,
		progress:
			nodesDone !== null && nodesTotal && nodesTotal > 0
				? Math.min(1, nodesDone / nodesTotal)
				: null,
		durationMs:
			startedAt !== null && completedAt !== null
				? completedAt - startedAt
				: null,
		errorMessage: d.error ?? null,
	};
}

interface ToolCallDto {
	id?: string;
	name?: string;
	kind?: string;
	status?: string;
	started_at?: string;
	duration_ms?: number;
	input?: string;
	output?: string;
}

interface TimelineDto {
	id?: string;
	at?: string;
	kind?: string;
	title?: string;
	detail?: string;
	status?: string;
}

function toToolCall(d: ToolCallDto): ToolCallEntry {
	return {
		id: d.id ?? '',
		name: d.name ?? '',
		kind: d.kind ?? '',
		status: d.status ?? '',
		startedAt: d.started_at ?? '',
		durationMs: d.duration_ms ?? 0,
		input: d.input ?? '',
		output: d.output ?? '',
	};
}

function toTimelineEntry(d: TimelineDto): TimelineEntry {
	return {
		id: d.id ?? '',
		at: d.at ?? '',
		kind: d.kind ?? '',
		title: d.title ?? '',
		detail: d.detail ?? '',
		status: d.status ?? '',
	};
}

export async function listExecutions(params?: {
	limit?: number;
	offset?: number;
	workflowId?: string;
}): Promise<PageResult<Execution>> {
	const { workflowId, ...page } = params ?? {};
	const data = await call<unknown>(
		client.GET('/api/v1/executions', {
			params: { query: { ...page, workflow_id: workflowId } },
		}),
	);
	const result = extractPage<ExecutionDto>(data);
	return { ...result, items: result.items.map(toExecution) };
}

export async function getExecution(id: string): Promise<ExecutionDetail> {
	const data = await call<ExecutionDto>(
		client.GET('/api/v1/executions/{id}', {
			params: { path: { id } },
		}),
	);
	return {
		...toExecution(data),
		variables: (data.variables ?? []).map(toVariable),
		input: asJsonText(data.input),
		output: asJsonText(data.output),
		failures: data.errors ?? [],
		executionType: data.execution_type ?? null,
	};
}

export async function listToolCalls(
	executionId: string,
): Promise<ToolCallEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/audit/tool-calls', {
			params: { path: { id: executionId } },
		}),
	);
	if (Array.isArray(data)) {
		return (data as ToolCallDto[]).map(toToolCall);
	}
	const page = extractPage<ToolCallDto>(data);
	return page.items.map(toToolCall);
}

export async function listTimeline(
	executionId: string,
): Promise<TimelineEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/audit/timeline', {
			params: { path: { id: executionId } },
		}),
	);
	if (Array.isArray(data)) {
		return (data as TimelineDto[]).map(toTimelineEntry);
	}
	const page = extractPage<TimelineDto>(data);
	return page.items.map(toTimelineEntry);
}

export async function pauseExecution(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/executions/{id}/pause', { params: { path: { id } } }),
	);
}

export async function resumeExecution(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/executions/{id}/resume', { params: { path: { id } } }),
	);
}

export async function cancelExecution(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/executions/{id}/cancel', { params: { path: { id } } }),
	);
}
