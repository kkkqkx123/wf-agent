import { client, request } from '$lib/api/client';
import {
	call,
	extractCapped,
	extractPage,
	requireData,
} from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	AgentLoop,
	AgentLoopDetail,
	LoopIteration,
	LoopMessage,
	LoopVariable,
	TimelineEntry,
	ToolCallEntry,
	WorkflowGraph,
} from '$lib/types/models';
import {
	getDecisionGraph,
	getDecisionSteps,
	getDecisionToolFrequency,
	toDecisionGraph,
} from '$lib/services/graph';
import {
	inferToolKind,
	parseApprovalId,
	parseToolEndpoint,
	parseToolExitCode,
} from '$lib/utils/toolcalls';

interface SummaryDto {
	id?: string;
	status?: string;
	current_iteration?: number;
	tool_call_count?: number;
	start_time?: number | null;
	end_time?: number | null;
	execution_time?: number | null;
	profile_id?: string | null;
}

interface MessageContentPartDto {
	type?: string;
	text?: string;
	thinking?: string;
	tool_result?: { content?: string };
	tool_use?: { name?: string; input?: unknown };
}

interface MessageDto {
	id?: string;
	role?: string;
	content?: string | MessageContentPartDto[];
	timestamp?: number;
	tool_name?: string | null;
	thinking?: string | null;
}

interface ToolCallDto {
	name?: string;
	arguments?: unknown;
	result?: unknown;
	error?: string | null;
	tool_call_id?: string | null;
	duration_ms?: number;
	success?: boolean;
}

interface IterationDto {
	iteration?: number;
	start_time?: number;
	end_time?: number;
	duration?: number;
	tool_calls?: ToolCallDto[];
	response_content?: string | null;
}

interface TimelineDto {
	id?: string;
	timestamp?: number;
	type?: string;
	description?: string;
	error_severity?: string | null;
}

function toIso(value: number | null | undefined): string {
	return value === null || value === undefined
		? ''
		: new Date(value).toISOString();
}

function toLoop(d: SummaryDto): AgentLoop {
	const id = d.id ?? '';
	return {
		id,
		name: id,
		status: d.status ?? '',
		iteration: d.current_iteration ?? 0,
		maxIterations: d.current_iteration ?? 0,
		model: '',
		tokens: 0,
		toolCalls: d.tool_call_count ?? 0,
		durationMs: d.execution_time ?? null,
		profileId: d.profile_id ?? null,
		startedAt: toIso(d.start_time),
		updatedAt: toIso(d.end_time ?? d.start_time),
		endedAt: d.end_time ? toIso(d.end_time) : null,
		checkpoints: 0,
		errors: 0,
		starred: false,
		tags: [],
	};
}

/** Rich content parts collapse to the text the transcript can show. */
function flattenContent(
	content: string | MessageContentPartDto[] | undefined,
): string {
	if (typeof content === 'string') return content;
	if (!Array.isArray(content)) return '';
	return content
		.map((part) => {
			if (part.type === 'text') return part.text ?? '';
			if (part.type === 'tool_result') return part.tool_result?.content ?? '';
			if (part.type === 'tool_use') return part.tool_use?.name ?? '';
			return '';
		})
		.filter(Boolean)
		.join('\n');
}

function toMessage(d: MessageDto): LoopMessage {
	return {
		id: d.id ?? '',
		role: (d.role ?? 'assistant') as LoopMessage['role'],
		content: flattenContent(d.content),
		createdAt: toIso(d.timestamp),
		tokens: null,
		toolName: d.tool_name ?? null,
		thinking: d.thinking ?? null,
	} satisfies LoopMessage;
}

function toToolCall(d: ToolCallDto, iteration: IterationDto): ToolCallEntry {
	const name = d.name ?? '';
	const input = stringify(d.arguments);
	const output = d.error ?? stringify(d.result);
	return {
		id: d.tool_call_id ?? `${iteration.iteration ?? 0}-${d.name ?? ''}`,
		name,
		kind: inferToolKind(name),
		status: d.success ? 'completed' : 'failed',
		startedAt: toIso(iteration.start_time),
		durationMs: d.duration_ms ?? 0,
		iteration: iteration.iteration ?? undefined,
		input,
		output,
		endpoint: parseToolEndpoint(input) || undefined,
		exitCode: parseToolExitCode(output),
		approvalId: parseApprovalId(input, output) || undefined,
	};
}

function stringify(value: unknown): string {
	if (value === null || value === undefined) return '';
	return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

function toIteration(d: IterationDto): LoopIteration {
	return {
		index: d.iteration ?? 0,
		startedAt: toIso(d.start_time),
		durationMs: d.duration !== undefined && d.duration >= 0 ? d.duration : null,
		summary: d.response_content ?? '',
		toolCalls: (d.tool_calls ?? []).map((call) => toToolCall(call, d)),
	};
}

/**
 * The timeline enum carries the outcome in the entry type, so the badge reads
 * off that rather than off a status field the payload never has.
 */
function timelineStatus(
	type: string,
	severity: string | null | undefined,
): string {
	if (
		type.endsWith('_failed') ||
		type === 'error' ||
		type === 'execution_timeout'
	) {
		return severity ?? 'failed';
	}
	if (type === 'execution_completed') return 'completed';
	if (type === 'iteration_end') return 'completed';
	if (type === 'iteration_start') return 'running';
	if (type === 'interruption_pause') return 'paused';
	if (type === 'execution_cancelled' || type === 'execution_stopped') {
		return 'cancelled';
	}
	return 'started';
}

function toTimelineEntry(d: TimelineDto): TimelineEntry {
	const type = d.type ?? '';
	return {
		id: d.id ?? '',
		at: toIso(d.timestamp),
		kind: type,
		title: type.replace(/_/g, ' '),
		detail: d.description ?? '',
		status: timelineStatus(type, d.error_severity),
	};
}

/**
 * Decision graph mapped onto the shared topology model. Iteration columns
 * are computed by the canvas; the service only translates field names.
 */
export async function getLoopGraph(id: string): Promise<WorkflowGraph> {
	const view = await getDecisionGraph(id);
	return toDecisionGraph(view);
}

export async function listAgentLoops(params?: {
	limit?: number;
	offset?: number;
	status?: string;
}): Promise<PageResult<AgentLoop>> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/summaries', {
			params: { query: params ?? {} },
		}),
	);
	requireData(data, 'Agent loop list');
	const page = extractPage<SummaryDto>(data);
	return { ...page, items: page.items.map(toLoop) };
}

export async function getAgentLoop(id: string): Promise<AgentLoop> {
	const data = requireData(
		await call<SummaryDto>(
			client.GET('/api/v1/agent-loops/{id}/summary', {
				params: { path: { id } },
			}),
		),
		`Agent loop ${id}`,
	);
	return toLoop(data);
}

export async function listLoopMessages(
	id: string,
	params?: { limit?: number; offset?: number },
): Promise<PageResult<LoopMessage>> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/conversation', {
			params: { path: { id }, query: { limit: 500, ...params } },
		}),
	);
	requireData(data, `Conversation missing for loop ${id}`);
	const page = extractPage<MessageDto>(data);
	return { ...page, items: page.items.map(toMessage) };
}

export async function listLoopIterations(
	id: string,
	params?: { limit?: number; offset?: number },
): Promise<PageResult<LoopIteration>> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/iteration-history', {
			params: { path: { id }, query: params ?? {} },
		}),
	);
	requireData(data, `Iteration history missing for loop ${id}`);
	const page = extractPage<IterationDto>(data);
	return { ...page, items: page.items.map(toIteration) };
}

export async function listLoopTimeline(id: string): Promise<TimelineEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/timeline', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Timeline missing for loop ${id}`);
	return extractCapped<TimelineDto>(data).items.map(toTimelineEntry);
}

/** The loop variable endpoint answers with `[name, value]` pairs. */
export async function listLoopVariables(id: string): Promise<LoopVariable[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/variables', {
			params: { path: { id }, query: { limit: 200 } },
		}),
	);
	requireData(data, `Variables missing for loop ${id}`);
	return extractPage<[string, unknown]>(data).items.map(([key, value]) => ({
		key,
		type: 'string',
		value: stringify(value),
		scope: 'loop',
		updatedAt: '',
	}));
}

export async function pauseAgentLoop(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/agent-loops/{id}/pause', { params: { path: { id } } }),
	);
}

export async function resumeAgentLoop(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/agent-loops/{id}/resume', {
			params: { path: { id } },
		}),
	);
}

export async function cancelAgentLoop(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/agent-loops/{id}/cancel', {
			params: { path: { id } },
		}),
	);
}

export interface LoopErrorRecord {
	id: string;
	error: string;
	errorType: string | null;
	timestamp: number;
	nodeId: string | null;
	chain: string[];
	recoverable: boolean;
	recoveryAction: string | null;
}

interface ErrorRecordDto {
	id?: unknown;
	error?: unknown;
	error_type?: unknown;
	timestamp?: unknown;
	node_id?: unknown;
	error_chain?: unknown;
	is_recoverable?: unknown;
	recovery_action?: unknown;
}

function toErrorRecord(d: ErrorRecordDto): LoopErrorRecord {
	return {
		id: typeof d.id === 'string' ? d.id : '',
		error: typeof d.error === 'string' ? d.error : '',
		errorType: typeof d.error_type === 'string' ? d.error_type : null,
		timestamp: typeof d.timestamp === 'number' ? d.timestamp : 0,
		nodeId: typeof d.node_id === 'string' ? d.node_id : null,
		chain: Array.isArray(d.error_chain)
			? d.error_chain.filter(
					(entry): entry is string => typeof entry === 'string',
				)
			: [],
		recoverable: d.is_recoverable === true,
		recoveryAction:
			typeof d.recovery_action === 'string' ? d.recovery_action : null,
	};
}

/** Error chain of an agent loop execution, oldest first. */
export async function getLoopErrorChain(
	id: string,
): Promise<LoopErrorRecord[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-executions/{id}/errors/chain', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Error chain missing for loop ${id}`);
	const rows = Array.isArray(data) ? (data as ErrorRecordDto[]) : [];
	return rows.map(toErrorRecord);
}

export interface LoopRootCause {
	rootCauseId: string;
	error: string;
	chainLength: number;
	suggestedAction: string | null;
}

/** Root-cause analysis of an agent loop's error chain. */
export async function getLoopRootCause(id: string): Promise<LoopRootCause> {
	const data = await call<{
		root_cause_id?: unknown;
		error?: unknown;
		chain_length?: unknown;
		suggested_action?: unknown;
	} | null>(
		client.GET('/api/v1/agent-executions/{id}/errors/root-cause', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Root cause missing for loop ${id}`);
	return {
		rootCauseId:
			typeof data?.root_cause_id === 'string' ? data.root_cause_id : '',
		error: typeof data?.error === 'string' ? data.error : '',
		chainLength: typeof data?.chain_length === 'number' ? data.chain_length : 0,
		suggestedAction:
			typeof data?.suggested_action === 'string' ? data.suggested_action : null,
	};
}

/** Tool-call frequency for an agent loop, most used first. */
export async function getLoopToolFrequency(
	id: string,
): Promise<Array<{ tool: string; count: number }>> {
	return getDecisionToolFrequency(id);
}

/** Execution path steps of an agent loop. */
export async function getLoopDecisionSteps(id: string) {
	return getDecisionSteps(id);
}

/**
 * Untyped request: the generated types conflate the agent and workflow
 * checkpoint routes under one operation name.
 */
export async function createAgentLoopCheckpoint(
	id: string,
	description?: string,
): Promise<void> {
	await call<unknown>(
		request('POST', '/api/v1/agent-loops/{id}/checkpoints', {
			params: { path: { id } },
			body: { description: description ?? null },
		}),
	);
}

export async function restoreAgentLoopCheckpoint(
	id: string,
	checkpointId: string,
): Promise<void> {
	await call<unknown>(
		request('POST', '/api/v1/agent-loops/{id}/checkpoints/{cid}/restore', {
			params: { path: { id, cid: checkpointId } },
		}),
	);
}

export interface RunLoopMessage {
	id: string;
	role: string;
	content: string;
	timestamp: number;
}

export interface RunLoopInput {
	model: string;
	message: string;
	conversation?: RunLoopMessage[];
}

export interface AgentRunResult {
	agentLoopId: string;
	result: unknown;
	iterations: number;
}

interface AgentRunViewDto {
	agent_loop_id?: string;
	result?: unknown;
	iterations?: number;
}

/**
 * Start a loop run. The route keeps the `{id}` segment but the handler runs
 * from the body, so a draft composer passes `new` and adopts the returned
 * loop id as its session.
 */
export async function runAgentLoop(
	id: string,
	input: RunLoopInput,
): Promise<AgentRunResult> {
	const data = requireData(
		await call<AgentRunViewDto>(
			request('POST', '/api/v1/agent-loops/{id}/run', {
				params: { path: { id } },
				body: {
					model: input.model,
					message: input.message,
					tool_call_protocol: { format: 'json' },
					conversation: input.conversation ?? [],
				},
			}),
		),
		`Run result missing for loop ${id}`,
	);
	const agentLoopId = data.agent_loop_id ?? '';
	if (!agentLoopId) throw new Error(`Run result carries no loop id for ${id}`);
	return {
		agentLoopId,
		result: data.result,
		iterations: data.iterations ?? 0,
	};
}

/**
 * True detail aggregation: summary, decision graph, iterations and tool
 * frequency load together. Messages, variables and checkpoints stay
 * tab-lazy and are pulled on demand.
 */
export async function getAgentLoopDetail(id: string): Promise<AgentLoopDetail> {
	const [summary, graph, iterations, toolFrequency] = await Promise.all([
		getAgentLoop(id),
		getLoopGraph(id),
		listLoopIterations(id),
		getLoopToolFrequency(id),
	]);
	return {
		...summary,
		name: summary.name || id,
		summary: `${summary.status || 'unknown'} · iteration ${summary.iteration}`,
		messages: [],
		variables: [],
		iterations: iterations.items.map((iteration) => ({
			index: iteration.index,
			status: iteration.durationMs !== null ? 'completed' : 'running',
			durationMs: iteration.durationMs ?? 0,
			summary: iteration.summary,
		})),
		graph,
		analysis: {
			rootCause: null,
			errorChain: [],
			recoveryHints: [],
			toolFrequency,
		},
	};
}

export interface AgentLoopAnalysis {
	rootCause: string | null;
	errorChain: string[];
	recoveryHints: string[];
	toolFrequency: Array<{ tool: string; count: number }>;
}

/** Error analysis for the analysis tab: chain, root cause, hints. */
export async function getAgentLoopAnalysis(
	id: string,
): Promise<AgentLoopAnalysis> {
	const [chain, rootCause, toolFrequency] = await Promise.all([
		getLoopErrorChain(id),
		getLoopRootCause(id),
		getLoopToolFrequency(id),
	]);
	const hints = [
		...new Set(
			chain
				.map((record) => record.recoveryAction)
				.filter((hint): hint is string => !!hint),
		),
	];
	if (rootCause.suggestedAction && !hints.includes(rootCause.suggestedAction)) {
		hints.unshift(rootCause.suggestedAction);
	}
	return {
		rootCause: rootCause.error || null,
		errorChain: chain.map((record) => record.error || record.id),
		recoveryHints: hints,
		toolFrequency,
	};
}

/**
 * Conversation messages for an agent loop. The chat session reads through
 * this same mapping so both views interpret message payloads identically.
 * Each view still fetches on demand; there is no shared request cache.
 */
export async function getAgentLoopMessages(
	loopId: string,
): Promise<LoopMessage[]> {
	const page = await listLoopMessages(loopId);
	return page.items;
}

/** Variables snapshot for an agent loop. */
export async function getAgentLoopVariables(
	loopId: string,
): Promise<LoopVariable[]> {
	return listLoopVariables(loopId);
}
