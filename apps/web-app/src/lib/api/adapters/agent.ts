/**
 * Adapter: agent-loop endpoint payload → AgentLoop / AgentLoopDetail ViewModel.
 *
 * Backend `/agent-loops/summaries` returns `PageView { items: unknown[] }` —
 * schema.d.ts still types items as unknown.  This adapter is defensive and
 * fills every field with a sensible default.  As the backend adds typed
 * OpenAPI schemas for loop summaries, `pick*` calls will transparently
 * narrow to the proper types.
 *
 * Last schema.d.ts sync: 2026-09-24
 */
import type {
	AgentLoop,
	AgentLoopDetail,
	LoopVariable,
	LoopMessage,
	WorkflowGraph,
	GraphNode,
	GraphEdge,
} from '$lib/types/models';
import { pick, pickStr, pickNum, pickBool, pickArr, type AnyRecord } from './_utils';

export function adaptAgentLoop(raw: unknown): AgentLoop {
	const o = (raw ?? {}) as AnyRecord;
	const tags = pickArr<string>(o, 'tags');
	return {
		id: pickStr(o, 'id'),
		name: pickStr(o, 'name'),
		status: pickStr(o, 'status', 'unknown'),
		iteration: pickNum(o, 'iteration', 0)!,
		maxIterations: pickNum(o, 'max_iterations', 0)!,
		model: pickStr(o, 'model'),
		tokens: pickNum(o, 'tokens', 0)!,
		startedAt: pickStr(o, 'started_at'),
		updatedAt: pickStr(o, 'updated_at'),
		checkpoints: pickNum(o, 'checkpoints', 0)!,
		errors: pickNum(o, 'errors', 0)!,
		starred: pickBool(o, 'starred'),
		tags,
	};
}

export function adaptLoopMessage(raw: unknown): LoopMessage {
	const o = (raw ?? {}) as AnyRecord;
	return {
		id: pickStr(o, 'id'),
		role: pickStr(o, 'role', 'assistant') as LoopMessage['role'],
		content: pickStr(o, 'content'),
		createdAt: pickStr(o, 'created_at'),
		tokens: pickNum(o, 'tokens', null),
		toolName: pickStr(o, 'tool_name', undefined as unknown as string) || undefined,
	};
}

export function adaptLoopVariable(raw: unknown): LoopVariable {
	const o = (raw ?? {}) as AnyRecord;
	return {
		key: pickStr(o, 'key'),
		type: pickStr(o, 'type'),
		value: String(pick<unknown>(o, 'value', '') ?? ''),
		scope: pickStr(o, 'scope', 'loop'),
		updatedAt: pickStr(o, 'updated_at'),
	};
}

export function adaptGraph(raw: unknown): WorkflowGraph {
	if (!raw || typeof raw !== 'object') return { nodes: [], edges: [] };
	const o = raw as AnyRecord;

	const nodes: GraphNode[] = pickArr<AnyRecord>(o, 'nodes').map((n) => ({
		id: pickStr(n, 'id'),
		label: pickStr(n, 'label'),
		kind: pickStr(n, 'kind', 'task'),
		status: pickStr(n, 'status', undefined as unknown as string) || undefined,
		x: pickNum(n, 'x', 0)!,
		y: pickNum(n, 'y', 0)!,
	}));

	const edges: GraphEdge[] = pickArr<AnyRecord>(o, 'edges').map((e) => ({
		id: pickStr(e, 'id'),
		from: pickStr(e, 'from'),
		to: pickStr(e, 'to'),
		label: pickStr(e, 'label', undefined as unknown as string) || undefined,
	}));

	return { nodes, edges };
}

export function adaptAgentLoopDetail(
	loop: unknown,
	extras?: {
		messages?: unknown;
		variables?: unknown;
		graph?: unknown;
		iterations?: unknown;
		summary?: unknown;
	},
): AgentLoopDetail {
	const base = adaptAgentLoop(loop);
	const o = (loop ?? {}) as AnyRecord;
	const summary = extras?.summary ?? pick<unknown>(o, 'summary', null);
	const messagesArr: unknown[] =
		(extras?.messages as unknown[]) ?? pickArr(o, 'messages');
	const variablesArr: unknown[] =
		(extras?.variables as unknown[]) ?? pickArr(o, 'variables');
	const graphRaw = extras?.graph ?? pick<unknown>(o, 'graph', null);
	const iterationsArr: unknown[] =
		(extras?.iterations as unknown[]) ?? pickArr(o, 'iterations');

	return {
		...base,
		summary: typeof summary === 'string' ? summary : pickStr(o, 'summary'),
		messages: messagesArr.map(adaptLoopMessage),
		variables: variablesArr.map(adaptLoopVariable),
		iterations: iterationsArr.map((it) => {
			const i = (it ?? {}) as AnyRecord;
			return {
				index: pickNum(i, 'index', 0)!,
				status: pickStr(i, 'status', 'unknown'),
				durationMs: pickNum(i, 'duration_ms', 0)!,
				summary: pickStr(i, 'summary'),
			};
		}),
		graph: adaptGraph(graphRaw),
		analysis: {
			rootCause:
				((pick<unknown>(o, 'root_cause', null) ??
					pickStr(
						(o.analysis as AnyRecord) || {},
						'root_cause',
						null as unknown as string,
					)) ||
					null) as string | null,
			errorChain: pickArr<string>((o.analysis as AnyRecord) || {}, 'error_chain'),
			recoveryHints: pickArr<string>((o.analysis as AnyRecord) || {}, 'recovery_hints'),
			toolFrequency: pickArr<AnyRecord>((o.analysis as AnyRecord) || {}, 'tool_frequency').map(
				(t) => ({ tool: pickStr(t, 'tool'), count: pickNum(t, 'count', 0)! }),
			),
		},
	};
}
