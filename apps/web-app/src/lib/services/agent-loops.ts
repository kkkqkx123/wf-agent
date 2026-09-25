import { client, request } from '$lib/api/client';
import { call, extractCapped, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	AgentLoop,
	LoopIteration,
	LoopMessage,
	LoopVariable,
	TimelineEntry,
	ToolCallEntry,
	WorkflowGraph,
} from '$lib/types/models';

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

interface DecisionNodeDto {
	node_id?: string;
	type?: string;
	description?: string;
	iteration?: number;
}

interface DecisionEdgeDto {
	edge_id?: string;
	from_node_id?: string;
	to_node_id?: string;
	reason?: string | null;
	condition?: string | null;
}

function toIso(value: number | null | undefined): string {
	return value === null || value === undefined
		? ''
		: new Date(value).toISOString();
}

function toLoop(d: SummaryDto): AgentLoop {
	return {
		id: d.id ?? '',
		status: d.status ?? '',
		iteration: d.current_iteration ?? 0,
		toolCalls: d.tool_call_count ?? 0,
		durationMs: d.execution_time ?? null,
		profileId: d.profile_id ?? null,
		startedAt: toIso(d.start_time),
		endedAt: d.end_time ? toIso(d.end_time) : null,
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
		thinking: d.thinking ?? null,
		createdAt: toIso(d.timestamp),
		toolName: d.tool_name ?? null,
	} satisfies LoopMessage;
}

function toToolCall(d: ToolCallDto, iteration: IterationDto): ToolCallEntry {
	return {
		id: d.tool_call_id ?? `${iteration.iteration ?? 0}-${d.name ?? ''}`,
		name: d.name ?? '',
		kind: '',
		status: d.success ? 'completed' : 'failed',
		startedAt: toIso(iteration.start_time),
		durationMs: d.duration_ms ?? 0,
		input: stringify(d.arguments),
		output: d.error ?? stringify(d.result),
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
 * The decision graph has no coordinates, so nodes are laid out in one column
 * per iteration and the canvas keeps the run order readable.
 */
function toGraph(
	view:
		| {
				nodes?: DecisionNodeDto[];
				edges?: DecisionEdgeDto[];
				error_node_ids?: string[];
		  }
		| undefined,
): WorkflowGraph {
	const errorIds = new Set(view?.error_node_ids ?? []);
	const COLUMN = 190;
	const ROW = 62;
	const byIteration = new Map<number, number>();
	return {
		nodes: (view?.nodes ?? []).map((node) => {
			const iteration = node.iteration ?? 0;
			const row = byIteration.get(iteration) ?? 0;
			byIteration.set(iteration, row + 1);
			return {
				id: node.node_id ?? '',
				label: node.description ?? node.node_id ?? '',
				kind: node.type ?? 'decision',
				status: errorIds.has(node.node_id ?? '') ? 'failed' : undefined,
				x: 24 + iteration * COLUMN,
				y: 24 + row * ROW,
			};
		}),
		edges: (view?.edges ?? []).map((edge) => ({
			id: edge.edge_id ?? `${edge.from_node_id ?? ''}-${edge.to_node_id ?? ''}`,
			from: edge.from_node_id ?? '',
			to: edge.to_node_id ?? '',
			label: edge.reason ?? edge.condition ?? undefined,
		})),
	};
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
	const page = extractPage<SummaryDto>(data);
	return { ...page, items: page.items.map(toLoop) };
}

export async function getAgentLoop(id: string): Promise<AgentLoop> {
	const data = await call<SummaryDto>(
		client.GET('/api/v1/agent-loops/{id}/summary', {
			params: { path: { id } },
		}),
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
	const page = extractPage<IterationDto>(data);
	return { ...page, items: page.items.map(toIteration) };
}

export async function getLoopGraph(id: string): Promise<WorkflowGraph> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/graph', { params: { path: { id } } }),
	);
	return toGraph(
		data as
			| {
					nodes?: DecisionNodeDto[];
					edges?: DecisionEdgeDto[];
					error_node_ids?: string[];
			  }
			| undefined,
	);
}

export async function listLoopTimeline(id: string): Promise<TimelineEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/timeline', {
			params: { path: { id } },
		}),
	);
	return extractCapped<TimelineDto>(data).items.map(toTimelineEntry);
}

/** The loop variable endpoint answers with `[name, value]` pairs. */
export async function listLoopVariables(id: string): Promise<LoopVariable[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/variables', {
			params: { path: { id }, query: { limit: 200 } },
		}),
	);
	return extractPage<[string, unknown]>(data).items.map(([key, value]) => ({
		key,
		value: stringify(value),
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

/**
 * utoipa emits one `handle_create_checkpoint` name for both the agent and the
 * workflow route, so the generated types describe the wrong signature and the
 * untyped `request()` helper is used instead.
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
	const data = await call<AgentRunViewDto>(
		request('POST', '/api/v1/agent-loops/{id}/run', {
			params: { path: { id } },
			body: {
				model: input.model,
				message: input.message,
				tool_call_protocol: { format: 'json' },
				conversation: input.conversation ?? [],
			},
		}),
	);
	return {
		agentLoopId: data.agent_loop_id ?? '',
		result: data.result,
		iterations: data.iterations ?? 0,
	};
}
