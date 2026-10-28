import { client } from '#lib/api/client';
import type { components } from '#lib/api/schema';
import { call, extractCursorPage, extractPage, requireData } from '#lib/api/envelope';
import type { CursorPageResult } from '#lib/api/envelope';
import type {
	Execution,
	ExecutionHierarchy,
	ExecutionHistory,
	ExecutionKind,
	ExecutionRef,
	ExecutionSubtree,
	ExecutionSubtreeNode,
	IterationRecord,
	Metric,
	NodeExecution,
	TimelineEntry,
	ToolCallEntry,
} from '#lib/types/models';
import {
	inferToolKind,
	parseApprovalId,
	parseToolEndpoint,
	parseToolExitCode,
} from '#lib/utils/toolcalls';
import { inferDisplayKind } from '#lib/utils/tool-output';

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

function stringify(value: unknown): string {
	if (value === null || value === undefined) return '';
	return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

interface ToolCallDto {
	id?: string;
	name?: string;
	tool?: string;
	node_id?: string;
	iteration?: number;
	kind?: string;
	status?: string;
	success?: boolean;
	started_at?: number;
	duration_ms?: number;
	input?: unknown;
	arguments?: unknown;
	output?: unknown;
	result?: unknown;
	error?: string | null;
}

function toToolCall(d: ToolCallDto, index: number): ToolCallEntry {
	const name = d.name ?? d.tool ?? '';
	const kind = d.kind ?? inferToolKind(name);
	const input = typeof d.input === 'string' ? d.input : stringify(d.input ?? d.arguments);
	const output =
		typeof d.output === 'string' ? d.output : (d.error ?? stringify(d.output ?? d.result));
	return {
		id: d.id ?? `tc-${index}`,
		name,
		kind,
		nodeId: d.node_id ?? undefined,
		iteration: typeof d.iteration === 'number' ? d.iteration : undefined,
		status: d.status ?? (d.success === false ? 'failed' : d.success === true ? 'completed' : ''),
		startedAt: toIso(d.started_at),
		durationMs: d.duration_ms ?? 0,
		input,
		output,
		error: d.error ?? undefined,
		displayKind: inferDisplayKind(name),
		endpoint: parseToolEndpoint(input) || undefined,
		exitCode: parseToolExitCode(output),
		approvalId: parseApprovalId(input, output) || undefined,
	};
}

/** Tool calls for an execution from the unified audit view. */
export async function getExecutionToolCalls(executionId: string): Promise<ToolCallEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/audit/tool-calls', {
			params: { path: { id: executionId } },
		}),
	);
	requireData(data, `Tool calls missing for execution ${executionId}`);
	const page = extractPage<ToolCallDto>(data);
	return page.items.map((d, index) => toToolCall(d, index));
}

interface UnifiedExecutionDto {
	execution_id?: string;
	execution_type?: string;
	status?: string;
	start_time?: number;
	end_time?: number | null;
	definition_id?: string | null;
	version?: string | null;
	parent_execution_id?: string | null;
	rerun_of?: string | null;
	debug?: boolean | null;
	error?: string | null;
}

function toUnifiedExecution(d: UnifiedExecutionDto): Execution {
	const status = String(d.status ?? 'running').toLowerCase();
	const completed = status === 'completed' || status === 'succeeded' || status === 'success';
	const kind = toExecutionKind(d.execution_type ?? 'workflow');
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
		version: d.version ?? null,
		error: d.error ?? null,
		parentExecutionId: d.parent_execution_id ?? null,
		rerunOf: d.rerun_of ?? null,
		debug: typeof d.debug === 'boolean' ? d.debug : null,
		kind,
	};
}

/** Cross-engine execution listing, newest first, with cursor paging. */
export async function listUnifiedExecutions(params?: {
	limit?: number;
	cursor?: string;
	status?: string;
	executionType?: string;
	definitionId?: string;
}): Promise<CursorPageResult<Execution>> {
	const data = await call<unknown>(
		client.GET('/api/v1/unified-executions', {
			params: {
				query: {
					limit: params?.limit,
					cursor: params?.cursor,
					status: params?.status,
					execution_type: params?.executionType,
					definition_id: params?.definitionId,
				},
			},
		}),
	);
	const page = extractCursorPage<UnifiedExecutionDto>(requireData(data, 'Unified execution list'));
	return { ...page, items: page.items.map(toUnifiedExecution) };
}

interface UnifiedExecutionDetailDto {
	execution_id?: string;
	execution_type?: string;
	status?: string;
	start_time?: number;
	end_time?: number | null;
	definition_id?: string | null;
	version?: string | null;
	rerun_of?: string | null;
	parent_execution_id?: string | null;
	debug?: boolean | null;
	error?: string | null;
	record?: unknown;
}

/**
 * One execution across engines, tagged with its real executor. The detail
 * carries the owning engine's raw record as `record`; a chat detail carries
 * the session id as its id, and the caller reads its messages through the
 * conversation endpoint.
 */
export interface UnifiedExecutionDetail {
	executionId: string;
	executionType: ExecutionKind;
	status: string;
	startTime: string;
	endTime: string | null;
	definitionId: string | null;
	version: string | null;
	rerunOf: string | null;
	debug: boolean | null;
	parentExecutionId: string | null;
	error: string | null;
	record: unknown;
}

/** Detail of one execution across engines, dispatched by the backend. */
export async function getUnifiedExecutionDetail(
	executionId: string,
): Promise<UnifiedExecutionDetail> {
	const id = requireExecutionId(executionId);
	const data = requireData(
		await call<UnifiedExecutionDetailDto>(
			client.GET('/api/v1/unified-executions/{id}', {
				params: { path: { id } },
			}),
		),
		`Unified execution ${id}`,
	);
	return {
		executionId: data.execution_id ?? id,
		executionType: toExecutionKind(data.execution_type ?? 'workflow'),
		status: String(data.status ?? 'running'),
		startTime: toIso(data.start_time),
		endTime: toIsoOrNull(data.end_time),
		definitionId: data.definition_id ?? null,
		version: typeof data.version === 'string' && data.version !== '' ? data.version : null,
		rerunOf: typeof data.rerun_of === 'string' && data.rerun_of !== '' ? data.rerun_of : null,
		debug: typeof data.debug === 'boolean' ? data.debug : null,
		parentExecutionId: data.parent_execution_id ?? null,
		error: data.error ?? null,
		record: data.record ?? null,
	};
}

/**
 * Overview metric cards across every engine. Counts reflect the unified
 * list page rather than a server total.
 */
export async function getUnifiedExecutionStats(): Promise<Metric[]> {
	const page = await listUnifiedExecutions({ limit: 200 });
	const running = page.items.filter((e) => e.status === 'running').length;
	const completed = page.items.filter((e) => e.status === 'completed').length;
	const failed = page.items.filter((e) => e.status === 'failed').length;
	return [
		{ label: 'Running', value: String(running), tone: 'running' },
		{ label: 'Completed', value: String(completed), tone: 'success' },
		{ label: 'Failed', value: String(failed), tone: 'danger' },
	];
}

/**
 * Rerun capability declared per execution type. Only workflow runs replay
 * through `rerunWorkflowRun`; agent and chat runs have no rerun endpoint,
 * so callers render an explicit disabled state instead of hiding the action.
 */
export function isRerunnableExecutionType(kind: ExecutionKind | undefined): boolean {
	return kind === undefined || kind === 'workflow';
}

/** Display reason when rerun is unavailable for an execution type. */
export function describeRerunUnsupported(kind: ExecutionKind): string {
	if (kind === 'agent') return 'Agent rerun is not supported yet';
	if (kind === 'chat') return 'Chat transcripts are read-only and cannot rerun';
	return 'Rerun is only available for workflow runs';
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
	return extractCursorPage<LogEntryDto>(requireData(data, `Logs missing for execution ${id}`));
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
type NodeExecutionDoc = components['schemas']['NodeExecutionDoc'];
type StatusTransitionDoc = components['schemas']['StateTransitionDoc'];

/**
 * The wire carries `ExecutionType` as a plain string naming the executor that
 * ran it. The client knows the executors that exist and refuses to file a run
 * under one it does not recognise, rather than guessing the majority case
 * and mislabelling the run.
 */
function toExecutionKind(value: string): ExecutionKind {
	if (value === 'workflow') return 'workflow';
	if (value === 'agent') return 'agent';
	if (value === 'chat') return 'chat';
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
export async function getExecutionHierarchy(executionId: string): Promise<ExecutionHierarchy> {
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
export async function getExecutionSubtree(executionId: string): Promise<ExecutionSubtree> {
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
			typeof data.next_cursor === 'string' && data.next_cursor !== '' ? data.next_cursor : null;
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

/** Names accepted by the injection application policy. */
export type VariableInjectionPolicy = 'next_boundary' | 'on_resume';

/** One buffered variable update waiting for its application policy. */
export interface VariableInjection {
	injectionId: string;
	variableName: string;
	value: unknown;
	source: string;
	submittedAt: number;
	policy: string;
}

type VariableInjectionViewDoc = components['schemas']['VariableInjectionView'];

function toVariableInjection(d: VariableInjectionViewDoc): VariableInjection {
	return {
		injectionId: d.injection_id,
		variableName: d.variable_name,
		value: d.value,
		source: d.source,
		submittedAt: d.submitted_at,
		policy: d.policy,
	};
}

/**
 * Buffer an externally submitted variable update on a live workflow
 * execution. The update waits for its policy instead of touching the
 * running nodes. Failures throw so the caller can keep the unsent draft;
 * nothing is cleared here.
 */
export async function submitVariableInjection(
	executionId: string,
	input: {
		variableName: string;
		value: unknown;
		source?: string;
		policy?: VariableInjectionPolicy;
	},
): Promise<VariableInjection> {
	const id = requireExecutionId(executionId);
	const data = requireData(
		await call<VariableInjectionViewDoc>(
			client.POST('/api/v1/executions/{id}/variable-injections', {
				params: { path: { id } },
				body: {
					variable_name: input.variableName,
					value: input.value,
					source: input.source ?? null,
					policy: input.policy ?? null,
				},
			}),
		),
		`Variable injection missing for execution ${id}`,
	);
	return toVariableInjection(data);
}

/** Pending variable updates of a workflow execution, oldest first. */
export async function listVariableInjections(executionId: string): Promise<VariableInjection[]> {
	const id = requireExecutionId(executionId);
	const data = requireData(
		await call<{ injections?: VariableInjectionViewDoc[] }>(
			client.GET('/api/v1/executions/{id}/variable-injections', {
				params: { path: { id } },
			}),
		),
		`Variable injections missing for execution ${id}`,
	);
	return (data.injections ?? []).map(toVariableInjection);
}

/**
 * Drop one pending update. Applied values are never removed this way;
 * only the buffered entry goes away. Unknown ids report NotFound.
 */
export async function removeVariableInjection(
	executionId: string,
	injectionId: string,
): Promise<void> {
	const id = requireExecutionId(executionId);
	if (injectionId === '') {
		throw new Error('Injection id must not be empty');
	}
	await call<unknown>(
		client.DELETE('/api/v1/executions/{id}/variable-injections/{injection_id}', {
			params: { path: { id, injection_id: injectionId } },
		}),
	);
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

/** Owning graph node id carried in event metadata, if any. */
function metadataNodeId(metadata: Record<string, unknown> | undefined): string | undefined {
	if (!metadata) return undefined;
	const value = metadata.node_id;
	if (typeof value === 'string' && value !== '') return value;
	return undefined;
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

function toNodeExecution(record: NodeExecutionDoc): NodeExecution {
	return {
		nodeId: record.node_id,
		nodeType: record.node_type,
		nodeName: record.node_name ?? null,
		startedAt: toIso(record.started_at),
		endedAt: record.completed_at != null ? toIso(record.completed_at) : null,
		durationMs: record.duration_ms,
		error: record.error ?? null,
		input: record.input ?? null,
		result: record.result ?? null,
		metadata: record.metadata ?? null,
	};
}

/**
 * Everything one execution recorded, grouped by section. `include` narrows
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
		nodes: sectionRows(data.node_executions).map(toNodeExecution),
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
