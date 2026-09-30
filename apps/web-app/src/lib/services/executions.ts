import { client } from '$lib/api/client';
import {
	call,
	extractCapped,
	extractPage,
	requireData,
} from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	Execution,
	ExecutionDetail,
	Metric,
	TimelineEntry,
	ToolCallEntry,
} from '$lib/types/models';
import {
	inferToolKind,
	parseApprovalId,
	parseToolEndpoint,
	parseToolExitCode,
} from '$lib/utils/toolcalls';

interface ExecutionDto {
	id?: string;
	workflow_id?: string;
	workflowId?: string;
	workflow_name?: string;
	workflowName?: string;
	status?: string;
	started_at?: number;
	startedAt?: string;
	completed_at?: number | null;
	endedAt?: string | null;
	elapsed_ms?: number | null;
	durationMs?: number | null;
	error_count?: number;
	failedNodes?: number;
	error?: string | null;
	progress?: number;
	current_node?: string | null;
	currentNode?: string | null;
	trigger?: string | null;
	tasks_total?: number;
	tasksTotal?: number;
	tasks_done?: number;
	tasksDone?: number;
	memory_peak_bytes?: number | null;
	memoryPeakBytes?: number | null;
	input?: unknown;
}

function toIso(value: number | string | null | undefined): string {
	if (value === null || value === undefined) return '';
	if (typeof value === 'string') return value;
	return new Date(value).toISOString();
}

function toIsoOrNull(value: number | string | null | undefined): string | null {
	if (value === null || value === undefined) return null;
	if (typeof value === 'string') return value;
	return new Date(value).toISOString();
}

function toExecution(d: ExecutionDto): Execution {
	const status = String(d.status ?? 'running').toLowerCase();
	const completed =
		status === 'completed' || status === 'succeeded' || status === 'success';
	return {
		id: d.id ?? '',
		workflowId: d.workflow_id ?? d.workflowId ?? '',
		workflowName:
			d.workflow_name ?? d.workflowName ?? d.workflow_id ?? d.workflowId ?? '',
		status,
		startedAt:
			typeof d.startedAt === 'string' ? d.startedAt : toIso(d.started_at),
		endedAt:
			typeof d.endedAt === 'string' ? d.endedAt : toIsoOrNull(d.completed_at),
		durationMs: d.durationMs ?? d.elapsed_ms ?? null,
		progress: d.progress ?? (completed ? 1 : 0),
		currentNode: d.current_node ?? d.currentNode ?? null,
		trigger: d.trigger ?? null,
		tasksTotal: d.tasks_total ?? d.tasksTotal ?? 0,
		tasksDone: d.tasks_done ?? d.tasksDone ?? (completed ? 1 : 0),
		failedNodes: d.failedNodes ?? d.error_count ?? 0,
		memoryPeakBytes: d.memory_peak_bytes ?? d.memoryPeakBytes ?? null,
		input: d.input,
		error: d.error ?? null,
	};
}

/** List executions with optional paging and filters. */
export async function listExecutions(params?: {
	limit?: number;
	offset?: number;
	status?: string;
	workflowId?: string;
}): Promise<PageResult<Execution>> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions', {
			params: {
				query: {
					limit: params?.limit,
					offset: params?.offset,
					status: params?.status,
					workflow_id: params?.workflowId,
				},
			},
		}),
	);
	const page = extractPage<ExecutionDto>(requireData(data, 'Execution list'));
	return { ...page, items: page.items.map((d) => toExecution(d)) };
}

/** Overview metric cards derived from the execution list. */
export async function getExecutionStats(): Promise<Metric[]> {
	const page = await listExecutions({ limit: 200 });
	const running = page.items.filter((e) => e.status === 'running').length;
	const completed = page.items.filter((e) => e.status === 'completed').length;
	const failed = page.items.filter((e) => e.status === 'failed').length;
	return [
		{ label: 'Running', value: String(running), tone: 'running' },
		{ label: 'Completed', value: String(completed), tone: 'success' },
		{ label: 'Failed', value: String(failed), tone: 'danger' },
	];
}

/** Detailed view of a single execution. */
export async function getExecutionDetail(id: string): Promise<ExecutionDetail> {
	const data = requireData(
		await call<ExecutionDto>(
			client.GET('/api/v1/executions/{id}', { params: { path: { id } } }),
		),
		`Execution ${id}`,
	);
	const base = toExecution(data);
	return {
		...base,
		id,
		context: [],
		callStack: [],
		variables: [],
		memory: { currentBytes: 0, peakBytes: base.memoryPeakBytes ?? 0 },
		migration: [],
		analysis: {
			slowNodes: [],
			decisionPoints: [],
			failureNodes: [],
			criticalPath: [],
			iterations: 0,
		},
	};
}

/** Compatibility alias for the detail route. */
export async function getExecution(id: string): Promise<ExecutionDetail> {
	return getExecutionDetail(id);
}

/** Pause a running execution. */
export async function pauseExecution(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/executions/{id}/pause', {
			params: { path: { id } },
		}),
	);
}

/** Resume a paused execution. */
export async function resumeExecution(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/executions/{id}/resume', {
			params: { path: { id } },
		}),
	);
}

/** Cancel a running execution. */
export async function cancelExecution(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/executions/{id}/cancel', {
			params: { path: { id } },
		}),
	);
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

/** Execution context entries. */
export async function getExecutionContext(
	id: string,
): Promise<Array<{ key: string; value: string }>> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/context', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Execution context missing for ${id}`);
	const rows = Array.isArray(data)
		? data
		: isRecord(data) && Array.isArray(data.entries)
			? data.entries
			: [];
	return rows.filter(isRecord).map((row) => ({
		key: String(row.key ?? row.name ?? ''),
		value:
			typeof row.value === 'string'
				? row.value
				: JSON.stringify(row.value ?? null),
	}));
}

/** Execution variables. */
export async function getExecutionVariables(
	id: string,
): Promise<Array<{ key: string; value: string }>> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/variables', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Execution variables missing for ${id}`);
	const rows = Array.isArray(data)
		? data
		: isRecord(data) && Array.isArray(data.variables)
			? data.variables
			: [];
	return rows.filter(isRecord).map((row) => ({
		key: String(row.key ?? row.name ?? ''),
		value:
			typeof row.value === 'string'
				? row.value
				: JSON.stringify(row.value ?? null),
	}));
}

/** Execution call stack frames. */
export async function getExecutionCallStack(
	id: string,
): Promise<
	Array<{ node: string; depth: number; enteredAt: string; status: string }>
> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/call-stack', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Call stack missing for execution ${id}`);
	const rows = Array.isArray(data) ? data : [];
	return rows.filter(isRecord).map((row, index) => ({
		node: typeof row.node === 'string' ? row.node : `frame-${index}`,
		depth: typeof row.depth === 'number' ? row.depth : index,
		enteredAt:
			typeof row.entered_at === 'number'
				? new Date(row.entered_at).toISOString()
				: typeof row.enteredAt === 'string'
					? row.enteredAt
					: '',
		status: typeof row.status === 'string' ? row.status : 'unknown',
	}));
}

/** Execution memory snapshot. */
export async function getExecutionMemory(
	id: string,
): Promise<{ currentBytes: number; peakBytes: number }> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/memory', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Memory snapshot missing for execution ${id}`);
	if (!isRecord(data)) return { currentBytes: 0, peakBytes: 0 };
	const number = (key: string): number =>
		typeof data[key] === 'number' ? (data[key] as number) : 0;
	return {
		currentBytes: number('current_bytes') || number('currentBytes'),
		peakBytes: number('peak_bytes') || number('peakBytes'),
	};
}

interface ToolCallDto {
	id?: string;
	tool_call_id?: string;
	name?: string;
	tool?: string;
	node_id?: string;
	nodeId?: string;
	iteration?: number;
	kind?: string;
	status?: string;
	success?: boolean;
	started_at?: number;
	startedAt?: string;
	duration_ms?: number;
	durationMs?: number;
	input?: unknown;
	arguments?: unknown;
	output?: unknown;
	result?: unknown;
	error?: string | null;
}

function stringify(value: unknown): string {
	if (value === null || value === undefined) return '';
	return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

function toToolCall(d: ToolCallDto, index: number): ToolCallEntry {
	const name = d.name ?? d.tool ?? '';
	const kind = d.kind ?? inferToolKind(name);
	const input =
		typeof d.input === 'string' ? d.input : stringify(d.input ?? d.arguments);
	const output =
		typeof d.output === 'string'
			? d.output
			: (d.error ?? stringify(d.output ?? d.result));
	return {
		id: d.id ?? d.tool_call_id ?? `tc-${index}`,
		name,
		kind,
		nodeId: d.node_id ?? d.nodeId ?? undefined,
		iteration: typeof d.iteration === 'number' ? d.iteration : undefined,
		status:
			d.status ??
			(d.success === false ? 'failed' : d.success === true ? 'completed' : ''),
		startedAt:
			typeof d.startedAt === 'string' ? d.startedAt : toIso(d.started_at),
		durationMs: d.duration_ms ?? d.durationMs ?? 0,
		input,
		output,
		endpoint: parseToolEndpoint(input) || undefined,
		exitCode: parseToolExitCode(output),
		approvalId: parseApprovalId(input, output) || undefined,
	};
}

/** Tool calls for an execution. */
export async function getExecutionToolCalls(
	executionId: string,
): Promise<ToolCallEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/audit/tool-calls', {
			params: { path: { id: executionId } },
		}),
	);
	requireData(data, `Tool calls missing for execution ${executionId}`);
	const page = extractPage<ToolCallDto>(data);
	return page.items.map((d, index) => toToolCall(d, index));
}

interface TimelineDto {
	id?: string;
	timestamp?: number;
	at?: string;
	type?: string;
	kind?: string;
	description?: string;
	title?: string;
	detail?: string;
	error_severity?: string | null;
	status?: string;
	metadata?: Record<string, unknown>;
}

/** Owning graph node id carried in event metadata, if any. Workflow node
 * lifecycle events always set `node_id`; other events carry none. */
function metadataNodeId(
	metadata: Record<string, unknown> | undefined,
): string | undefined {
	if (!metadata) return undefined;
	for (const key of ['node_id', 'nodeId', 'node']) {
		const value = metadata[key];
		if (typeof value === 'string' && value !== '') return value;
	}
	return undefined;
}

function toTimelineEntry(d: TimelineDto, index: number): TimelineEntry {
	const kind = d.kind ?? d.type ?? '';
	return {
		id: d.id ?? `tl-${index}`,
		at: typeof d.at === 'string' ? d.at : toIso(d.timestamp),
		kind,
		title: d.title ?? d.description ?? '',
		detail: d.detail ?? d.description ?? '',
		status: d.status ?? '',
		nodeId: metadataNodeId(d.metadata),
	};
}

/** Timeline events for an execution. */
export async function getExecutionTimeline(
	executionId: string,
): Promise<TimelineEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/events/execution-timeline/{executionId}', {
			params: { path: { executionId } },
		}),
	);
	requireData(data, `Timeline missing for execution ${executionId}`);
	const capped = extractCapped<TimelineDto>(data);
	const items =
		capped.items.length > 0
			? capped.items
			: extractPage<TimelineDto>(data).items;
	return items.map((d, index) => toTimelineEntry(d, index));
}

/** Filter executions by status. */
export async function filterExecutionsByStatus(
	status: string,
): Promise<Execution[]> {
	const page = await listExecutions({ status, limit: 200 });
	return page.items;
}
