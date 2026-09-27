import { client } from '$lib/api/client';
import { call, extractCapped, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { Execution, ExecutionDetail, Metric, TimelineEntry, ToolCallEntry } from '$lib/types/models';

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
	const completed = status === 'completed' || status === 'succeeded' || status === 'success';
	return {
		id: d.id ?? '',
		workflowId: d.workflow_id ?? d.workflowId ?? '',
		workflowName:
			d.workflow_name ?? d.workflowName ?? d.workflow_id ?? d.workflowId ?? '',
		status,
		startedAt: typeof d.startedAt === 'string' ? d.startedAt : toIso(d.started_at),
		endedAt:
			typeof d.endedAt === 'string' ? d.endedAt : toIsoOrNull(d.completed_at),
		durationMs: d.durationMs ?? d.elapsed_ms ?? null,
		progress: d.progress ?? (completed ? 1 : 0),
		currentNode: d.current_node ?? d.currentNode ?? null,
		trigger: d.trigger ?? null,
		tasksTotal: d.tasks_total ?? d.tasksTotal ?? 0,
		tasksDone: d.tasks_done ?? d.tasksDone ?? (completed ? 1 : 0),
		failedNodes: d.failedNodes ?? d.error_count ?? 0,
		memoryPeakBytes: d.memory_peak_bytes ?? d.memoryPeakBytes ?? null
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
					workflow_id: params?.workflowId
				}
			}
		})
	);
	const page = extractPage<ExecutionDto>(data);
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
		{ label: 'Failed', value: String(failed), tone: 'danger' }
	];
}

/** Detailed view of a single execution. */
export async function getExecutionDetail(id: string): Promise<ExecutionDetail> {
	const data = await call<ExecutionDto>(
		client.GET('/api/v1/executions/{id}', { params: { path: { id } } })
	);
	const base = toExecution(data ?? {});
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
			iterations: 0
		}
	};
}

/** Compatibility alias for the detail route. */
export async function getExecution(id: string): Promise<ExecutionDetail> {
	return getExecutionDetail(id);
}

interface ToolCallDto {
	id?: string;
	tool_call_id?: string;
	name?: string;
	tool?: string;
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
	return {
		id: d.id ?? d.tool_call_id ?? `tc-${index}`,
		name: d.name ?? d.tool ?? '',
		kind: d.kind ?? '',
		status: d.status ?? (d.success === false ? 'failed' : d.success === true ? 'completed' : ''),
		startedAt: typeof d.startedAt === 'string' ? d.startedAt : toIso(d.started_at),
		durationMs: d.duration_ms ?? d.durationMs ?? 0,
		input: typeof d.input === 'string' ? d.input : stringify(d.input ?? d.arguments),
		output: typeof d.output === 'string' ? d.output : (d.error ?? stringify(d.output ?? d.result))
	};
}

/** Tool calls for an execution. */
export async function getExecutionToolCalls(executionId: string): Promise<ToolCallEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/audit/tool-calls', {
			params: { path: { id: executionId } }
		})
	);
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
}

function toTimelineEntry(d: TimelineDto, index: number): TimelineEntry {
	const kind = d.kind ?? d.type ?? '';
	return {
		id: d.id ?? `tl-${index}`,
		at: typeof d.at === 'string' ? d.at : toIso(d.timestamp),
		kind,
		title: d.title ?? d.description ?? '',
		detail: d.detail ?? d.description ?? '',
		status: d.status ?? ''
	};
}

/** Timeline events for an execution. */
export async function getExecutionTimeline(executionId: string): Promise<TimelineEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/events/execution-timeline/{executionId}', {
			params: { path: { executionId } }
		})
	);
	const capped = extractCapped<TimelineDto>(data);
	const items = capped.items.length > 0 ? capped.items : extractPage<TimelineDto>(data).items;
	return items.map((d, index) => toTimelineEntry(d, index));
}

/** Filter executions by status. */
export async function filterExecutionsByStatus(status: string): Promise<Execution[]> {
	const page = await listExecutions({ status, limit: 200 });
	return page.items;
}
