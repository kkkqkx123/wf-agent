import { client, request } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
	AgentLoop,
	AgentLoopDetail,
	LoopMessage,
	WorkflowGraph,
} from '$lib/types/models';

interface AgentLoopDto {
	id?: string;
	name?: string;
	status?: string;
	iteration?: number;
	max_iterations?: number;
	model?: string;
	tokens?: number;
	started_at?: string;
	updated_at?: string;
	checkpoints?: number;
	errors?: number;
	starred?: boolean;
	tags?: string[];
}

interface LoopMessageDto {
	id?: string;
	role?: string;
	content?: string;
	created_at?: string;
	tokens?: number | null;
	tool_name?: string;
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

function toAgentLoop(d: AgentLoopDto): AgentLoop {
	return {
		id: d.id ?? '',
		name: d.name ?? '',
		status: d.status ?? '',
		iteration: d.iteration ?? 0,
		maxIterations: d.max_iterations ?? 0,
		model: d.model ?? '',
		tokens: d.tokens ?? 0,
		startedAt: d.started_at ?? '',
		updatedAt: d.updated_at ?? '',
		checkpoints: d.checkpoints ?? 0,
		errors: d.errors ?? 0,
		starred: d.starred ?? false,
		tags: d.tags ?? [],
	};
}

function toMessage(d: LoopMessageDto): LoopMessage {
	return {
		id: d.id ?? '',
		role: (d.role ?? 'assistant') as LoopMessage['role'],
		content: d.content ?? '',
		createdAt: d.created_at ?? '',
		tokens: d.tokens ?? null,
		toolName: d.tool_name,
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

export async function listAgentLoops(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<AgentLoop>> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<AgentLoopDto>(data);
	return { ...page, items: page.items.map(toAgentLoop) };
}

export async function getAgentLoop(id: string): Promise<AgentLoopDetail> {
	const data = await call<AgentLoopDto>(
		client.GET('/api/v1/agent-loops/{id}', {
			params: { path: { id } },
		}),
	);
	const loop = toAgentLoop(data);

	const [summaryRes, messagesRes, graphRes] = await Promise.allSettled([
		call<{ summary?: string }>(
			client.GET('/api/v1/agent-loops/{id}/summary', {
				params: { path: { id } },
			}),
		),
		call<unknown>(
			client.GET('/api/v1/agent-loops/{id}/conversation', {
				params: { path: { id } },
			}),
		),
		call<unknown>(
			client.GET('/api/v1/agent-loops/{id}/graph', {
				params: { path: { id } },
			}),
		),
	]);

	const summary =
		summaryRes.status === 'fulfilled'
			? ((summaryRes.value as { summary?: string })?.summary ?? '')
			: '';
	const messages =
		messagesRes.status === 'fulfilled'
			? (Array.isArray(messagesRes.value)
					? messagesRes.value
					: extractPage<LoopMessageDto>(messagesRes.value).items
				).map((m) => toMessage(m as LoopMessageDto))
			: [];
	const graph =
		graphRes.status === 'fulfilled'
			? toGraph(
					graphRes.value as { nodes?: GraphNodeDto[]; edges?: GraphEdgeDto[] },
				)
			: { nodes: [], edges: [] };

	return {
		...loop,
		summary,
		variables: [],
		messages,
		iterations: [],
		graph,
		analysis: {
			rootCause: null,
			errorChain: [],
			recoveryHints: [],
			toolFrequency: [],
		},
	};
}

export async function pauseAgentLoop(id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/agent-loops/{id}/pause', {
			params: { path: { id } },
		}),
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
