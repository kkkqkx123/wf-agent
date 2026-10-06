import { client } from '$lib/api/client';
import type { components } from '$lib/api/schema';
import {
	call,
	extractCapped,
	extractCursorPage,
	extractPage,
	requireData,
} from '$lib/api/envelope';
import type { CursorPageResult, PageResult } from '$lib/api/envelope';
import type {
	Execution,
	ExecutionDetail,
	ExecutionHierarchy,
	ExecutionHistory,
	ExecutionKind,
	ExecutionRef,
	ExecutionSubtree,
	ExecutionSubtreeNode,
	IterationRecord,
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

/** Timeline events for an execution, following every cursor page so no
 * event is silently dropped past the page limit. */
export async function getExecutionTimeline(
	executionId: string,
): Promise<TimelineEntry[]> {
	const rows: TimelineEntry[] = [];
	let cursor: string | undefined = undefined;
	let index = 0;
	for (;;) {
		const data = await call<unknown>(
			client.GET('/api/v1/events/execution-timeline/{executionId}', {
				params: { path: { executionId }, query: { limit: 500, cursor } },
			}),
		);
		const view = requireData(
			data,
			`Timeline missing for execution ${executionId}`,
		) as {
			events?: TimelineDto[];
			next_cursor?: string | null;
			has_more?: boolean;
		};
		// Older backends answer a capped view; newer ones page the flat
		// event list. Both shapes accumulate the same way.
		const events = Array.isArray(view.events)
			? view.events
			: extractCapped<TimelineDto>(data).items;
		for (const event of events) {
			rows.push(toTimelineEntry(event, index));
			index += 1;
		}
		const next =
			typeof view.next_cursor === 'string' && view.next_cursor !== ''
				? view.next_cursor
				: null;
		if (next === null || events.length === 0) break;
		cursor = next;
		if (!view.has_more) break;
	}
	return rows;
}

/** Filter executions by status. */
export async function filterExecutionsByStatus(
	status: string,
): Promise<Execution[]> {
	const page = await listExecutions({ status, limit: 200 });
	return page.items;
}

interface UnifiedExecutionDto {
	execution_id?: string;
	execution_type?: string;
	status?: string;
	start_time?: number;
	end_time?: number | null;
	definition_id?: string | null;
	parent_execution_id?: string | null;
	error?: string | null;
}

function toUnifiedExecution(d: UnifiedExecutionDto): Execution {
	const status = String(d.status ?? 'running').toLowerCase();
	const completed =
		status === 'completed' || status === 'succeeded' || status === 'success';
	const kind = d.execution_type === 'agent_loop' ? 'agent_loop' : 'workflow';
	return {
		id: d.execution_id ?? '',
		workflowId: d.definition_id ?? '',
		workflowName: d.definition_id ?? d.execution_id ?? '',
		status,
		startedAt: toIso(d.start_time),
		endedAt: toIsoOrNull(d.end_time),
		durationMs: null,
		progress: completed ? 1 : 0,
		currentNode: null,
		trigger: null,
		tasksTotal: 0,
		tasksDone: completed ? 1 : 0,
		failedNodes: 0,
		memoryPeakBytes: null,
		error: d.error ?? null,
		kind,
	};
}

/** Cross-engine execution listing, newest first, with cursor paging. */
export async function listUnifiedExecutions(params?: {
	limit?: number;
	cursor?: string;
	status?: string;
	executionType?: string;
}): Promise<CursorPageResult<Execution>> {
	const data = await call<unknown>(
		client.GET('/api/v1/unified-executions', {
			params: {
				query: {
					limit: params?.limit,
					cursor: params?.cursor,
					status: params?.status,
					execution_type: params?.executionType,
				},
			},
		}),
	);
	const page = extractCursorPage<UnifiedExecutionDto>(
		requireData(data, 'Unified execution list'),
	);
	return { ...page, items: page.items.map(toUnifiedExecution) };
}

interface LogEntryDto {
	execution_id?: string | null;
	workflow_id?: string | null;
	timestamp?: number;
	event_type?: string;
	event_name?: string | null;
	message?: string;
}

/** Log entries of one execution, oldest first, with cursor paging. */
export async function getExecutionLogs(
	executionId: string,
	params?: { limit?: number; cursor?: string; message?: string },
): Promise<CursorPageResult<LogEntryDto>> {
	const id = requireExecutionId(executionId);
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/logs', {
			params: { path: { id }, query: params ?? {} },
		}),
	);
	return extractCursorPage<LogEntryDto>(
		requireData(data, `Logs missing for execution ${id}`),
	);
}

interface ArtifactEntryDto {
	execution_id?: string;
	execution_type?: string;
	name?: string;
	kind?: string;
	preview?: string;
	truncated?: boolean;
	size_bytes?: number;
}

/** Artifacts of one execution with cursor paging. */
export async function getExecutionArtifacts(
	executionId: string,
	params?: { limit?: number; cursor?: string; kind?: string },
): Promise<CursorPageResult<ArtifactEntryDto>> {
	const id = requireExecutionId(executionId);
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/artifacts', {
			params: { path: { id }, query: params ?? {} },
		}),
	);
	return extractCursorPage<ArtifactEntryDto>(
		requireData(data, `Artifacts missing for execution ${id}`),
	);
}

type HierarchyDoc = components['schemas']['HierarchyDoc'];
type ExecutionRefDoc = components['schemas']['ExecutionRefDoc'];
type SubtreeDoc = components['schemas']['SubtreeDoc'];
type SubtreeNodeDoc = components['schemas']['SubtreeNodeDoc'];
type HistoryDoc = components['schemas']['HistoryDoc'];
type HistoryEventDoc = components['schemas']['TimelineEventDoc'];
type IterationDoc = components['schemas']['IterationDoc'];
type StatusTransitionDoc = components['schemas']['StateTransitionDoc'];

/**
 * The wire carries `ExecutionType` as a plain string so a further engine can
 * ship without a schema rebuild. The client knows the engines that exist and
 * refuses to file a run under one it does not recognise, rather than
 * guessing the majority case and mislabelling the run.
 */
function toExecutionKind(value: string): ExecutionKind {
	if (value === 'workflow') return 'workflow';
	if (value === 'agent_loop') return 'agent_loop';
	throw new Error(`Unknown execution type: ${value}`);
}

/**
 * A non-empty execution id is the precondition of every id-parameterized
 * endpoint: an empty one builds a double-slash path and reaches an unrelated
 * route. Services state the precondition instead of letting the URL decide
 * what a missing id means.
 */
function requireExecutionId(executionId: string): string {
	if (executionId === '') {
		throw new Error('Execution id must not be empty');
	}
	return executionId;
}

function toExecutionRef(ref: ExecutionRefDoc): ExecutionRef {
	return {
		executionId: ref.execution_id,
		executionType: toExecutionKind(ref.execution_type),
	};
}

/** Where an execution sits in the parent/child tree of nested runs. */
export async function getExecutionHierarchy(
	executionId: string,
): Promise<ExecutionHierarchy> {
	const id = requireExecutionId(executionId);
	const data = requireData(
		await call<HierarchyDoc>(
			client.GET('/api/v1/executions/{id}/hierarchy', {
				params: { path: { id } },
			}),
		),
		`Hierarchy missing for execution ${id}`,
	);
	return {
		executionId: data.execution_id,
		executionType: toExecutionKind(data.execution_type),
		status: data.status,
		depth: data.depth,
		parent: data.parent ? toExecutionRef(data.parent) : null,
		root: toExecutionRef(data.root),
		ancestors: data.ancestors,
	};
}

/**
 * The view omits a field whose value is its own zero, so an absent field and
 * a zero field are one value on this wire rather than two states to pick
 * between.
 */
function toSubtreeNode(node: SubtreeNodeDoc): ExecutionSubtreeNode {
	return {
		executionId: node.execution_id,
		executionType: toExecutionKind(node.execution_type),
		status: node.status ?? null,
		depth: node.depth,
		parentExecutionId: node.parent_execution_id ?? null,
	};
}

/** Every execution below a root, breadth-first, following every cursor
 * page so wide trees arrive whole instead of clipped. */
export async function getExecutionSubtree(
	executionId: string,
): Promise<ExecutionSubtree> {
	const id = requireExecutionId(executionId);
	const nodes: ExecutionSubtreeNode[] = [];
	let cursor: string | undefined = undefined;
	let rootExecutionId: string | null = null;
	for (;;) {
		const data: SubtreeDoc & { next_cursor?: string | null } = requireData(
			await call<SubtreeDoc & { next_cursor?: string | null }>(
				client.GET('/api/v1/executions/{id}/subtree', {
					params: { path: { id }, query: { limit: 500, cursor } },
				}),
			),
			`Subtree missing for execution ${id}`,
		);
		rootExecutionId ??= data.root_execution_id;
		for (const node of data.nodes) {
			nodes.push(toSubtreeNode(node));
		}
		const next: string | null =
			typeof data.next_cursor === 'string' && data.next_cursor !== ''
				? data.next_cursor
				: null;
		if (next === null || !data.truncated) break;
		cursor = next;
	}
	return {
		rootExecutionId: rootExecutionId ?? id,
		truncated: false,
		omitted: 0,
		nodes,
	};
}

/** Names accepted by the history `include` parameter. */
export const HISTORY_SECTIONS = [
	'timeline',
	'nodes',
	'iterations',
	'variables',
	'context',
	'transitions',
] as const;

export type HistorySection = (typeof HISTORY_SECTIONS)[number];

/**
 * A sectioned read answers only for the sections it named: the rest come
 * back absent, and absent reads as empty here because an unrequested section
 * holds no data rather than unknown data.
 */
function sectionRows<T>(rows: T[] | undefined): T[] {
	return rows ?? [];
}

/**
 * One stored lifecycle event as a timeline row. These events carry no
 * display text of their own: the event type names the row, and the optional
 * secondary name refines it for custom events.
 */
function toHistoryTimelineEntry(event: HistoryEventDoc): TimelineEntry {
	const kind = event.event_name ?? event.type;
	return {
		id: event.id,
		at: toIso(event.timestamp),
		kind: event.type,
		title: kind,
		detail: kind,
		status: '',
		nodeId: metadataNodeId(event.metadata ?? undefined),
	};
}

function toIterationRecord(record: IterationDoc): IterationRecord {
	return {
		iteration: record.iteration,
		durationMs: record.duration,
		toolCallCount: record.tool_call_count,
		toolCalls: record.tool_calls.map((call) => ({
			name: call.name,
			durationMs: call.duration_ms,
			success: call.success,
		})),
		responseContent: record.response_content ?? null,
	};
}

/**
 * Everything an execution recorded, grouped by section. `include` narrows
 * what the backend loads; a section left out comes back empty.
 */
export async function getExecutionHistory(
	executionId: string,
	include?: HistorySection[],
): Promise<ExecutionHistory> {
	const id = requireExecutionId(executionId);
	const data = requireData(
		await call<HistoryDoc>(
			client.GET('/api/v1/executions/{id}/history', {
				params: {
					path: { id },
					query: include ? { include: include.join(',') } : {},
				},
			}),
		),
		`History missing for execution ${id}`,
	);
	return {
		executionId: data.execution_id,
		executionType: toExecutionKind(data.execution_type),
		timelineLimit: data.timeline_limit,
		timeline: sectionRows(data.timeline).map(toHistoryTimelineEntry),
		iterations: sectionRows(data.iterations).map(toIterationRecord),
		variables: Object.entries(data.variables ?? {}).map(([key, value]) => ({
			key,
			value: stringify(value),
		})),
		contextEvolution: sectionRows(data.context_evolution).map((step) => ({
			timestamp: step.timestamp,
			iteration: step.iteration,
			status: step.status,
			description: step.description,
			toolCalls: step.tool_calls ?? null,
		})),
		statusTransitions: sectionRows(data.status_transitions).map(
			(transition: StatusTransitionDoc) => ({
				from: transition.from,
				to: transition.to,
				timestamp: transition.timestamp,
			}),
		),
	};
}
