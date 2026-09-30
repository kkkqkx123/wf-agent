import { client } from '$lib/api/client';
import { call, requireData } from '$lib/api/envelope';
import type {
	LlmReasoningStep,
	NodeInputContext,
	NodeInputVariable,
	NodeTrace,
	NodeTraceToolDependency,
} from '$lib/types/models';

interface NodeTraceDto {
	execution_id?: string;
	node_id?: string;
	node_name?: string;
	node_type?: string;
	status?: string;
	start_time?: number;
	end_time?: number | null;
	duration?: number | null;
	input?: unknown;
	output?: unknown;
	retry_count?: number;
	error?: string | null;
	tool_dependencies?: Array<{ tool_name?: string; call_count?: number }>;
}

interface NodeInputContextDto {
	node_id?: string;
	node_name?: string;
	node_type?: string;
	input_parameters?: Record<string, unknown>;
	available_variables?: Array<{
		name?: string;
		value?: unknown;
		type?: string;
		source?: string | null;
	}>;
	timestamp?: number;
}

interface LlmReasoningDto {
	step_id?: string;
	reasoning_type?: string;
	content?: string;
	confidence?: number | null;
	conclusions?: string[];
}

/** Node traces of an execution plus the rows dropped for a missing node id. */
export interface NodeTracePage {
	items: NodeTrace[];
	skipped: number;
}

function toIso(value: number | null | undefined): string {
	if (value === null || value === undefined) return '';
	return new Date(value).toISOString();
}

function toIsoOrNull(value: number | null | undefined): string | null {
	if (value === null || value === undefined) return null;
	return new Date(value).toISOString();
}

function stringify(value: unknown): string {
	if (value === null || value === undefined) return '';
	return typeof value === 'string' ? value : JSON.stringify(value);
}

/**
 * Map one backend record. Rows without a node id are unaddressable — they can
 * neither be located on the graph nor joined against other views — so they are
 * dropped rather than rendered under a synthetic id.
 */
export function toNodeTrace(row: NodeTraceDto): NodeTrace | null {
	if (typeof row.node_id !== 'string' || row.node_id.trim() === '') return null;
	const dependencies: NodeTraceToolDependency[] = Array.isArray(
		row.tool_dependencies,
	)
		? row.tool_dependencies.map((entry) => ({
				toolName: entry.tool_name ?? '',
				callCount: entry.call_count ?? 0,
			}))
		: [];
	return {
		executionId: row.execution_id ?? '',
		nodeId: row.node_id,
		nodeName: row.node_name ?? '',
		nodeType: row.node_type ?? '',
		status: row.status ?? '',
		startedAt: toIso(row.start_time),
		endedAt: toIsoOrNull(row.end_time),
		durationMs: row.duration ?? null,
		input: row.input ?? null,
		output: row.output ?? null,
		retryCount: row.retry_count ?? 0,
		error: row.error ?? null,
		toolDependencies: dependencies,
	};
}

export function toNodeInputContext(
	row: NodeInputContextDto,
): NodeInputContext | null {
	if (typeof row.node_id !== 'string' || row.node_id.trim() === '') return null;
	const parameters = row.input_parameters ?? {};
	const variables: NodeInputVariable[] = Array.isArray(row.available_variables)
		? row.available_variables.map((entry) => ({
				name: entry.name ?? '',
				value: stringify(entry.value),
				type: entry.type ?? '',
				source: entry.source ?? null,
			}))
		: [];
	return {
		nodeId: row.node_id,
		nodeName: row.node_name ?? '',
		nodeType: row.node_type ?? '',
		inputParameters: Object.entries(parameters).map(([key, value]) => ({
			key,
			value: stringify(value),
		})),
		availableVariables: variables,
		recordedAt: toIsoOrNull(row.timestamp),
	};
}

export function toLlmReasoningStep(
	row: LlmReasoningDto,
	index: number,
): LlmReasoningStep {
	return {
		stepId: row.step_id ?? `reasoning-${index}`,
		type: row.reasoning_type ?? '',
		content: row.content ?? '',
		confidence: row.confidence ?? null,
		conclusions: Array.isArray(row.conclusions) ? row.conclusions : [],
	};
}

/** Per-node execution records of an execution, in start-time order. */
export async function getExecutionNodeTraces(
	executionId: string,
): Promise<NodeTracePage> {
	const data = requireData(
		await call<unknown>(
			client.GET('/api/v1/executions/{id}/nodes', {
				params: { path: { id: executionId } },
			}),
		),
		`Node traces for ${executionId}`,
	);
	const rows = Array.isArray(data) ? (data as NodeTraceDto[]) : [];
	const items: NodeTrace[] = [];
	let skipped = 0;
	for (const row of rows) {
		const trace = toNodeTrace(row);
		if (trace) items.push(trace);
		else skipped += 1;
	}
	return { items, skipped };
}

/** Input parameters and variables available to a node when it executed. */
export async function getNodeInputContext(
	executionId: string,
	nodeId: string,
): Promise<NodeInputContext | null> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/nodes/{nodeId}/input-context', {
			params: { path: { id: executionId, nodeId } },
		}),
	);
	if (data === null || data === undefined) return null;
	return toNodeInputContext(data as NodeInputContextDto);
}

/** Reconstructed reasoning steps of an LLM node. */
export async function getNodeLlmReasoning(
	executionId: string,
	nodeId: string,
): Promise<LlmReasoningStep[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/llm-reasoning-path/{nodeId}', {
			params: { path: { id: executionId, nodeId } },
		}),
	);
	if (!Array.isArray(data)) return [];
	return (data as LlmReasoningDto[]).map(toLlmReasoningStep);
}

export const NODE_TRACE_STATUS_FILTERS = [
	{ value: 'all', label: 'All nodes' },
	{ value: 'failed', label: 'Failed' },
	{ value: 'completed', label: 'Completed' },
	{ value: 'running', label: 'Running' },
	{ value: 'skipped', label: 'Skipped' },
] as const;

export type NodeTraceStatusFilter =
	(typeof NODE_TRACE_STATUS_FILTERS)[number]['value'];

/** Statuses that count as a failed node across the backend vocabularies. */
const FAILED_STATUSES = ['failed', 'failure', 'error', 'errored', 'timeout'];

export function isFailedNodeTrace(trace: NodeTrace): boolean {
	return FAILED_STATUSES.includes(trace.status.trim().toLowerCase());
}

/**
 * Node rows after the status filter and free-text search. The search covers
 * id, name and type only — matching inside payloads would need the payloads
 * loaded, and they are fetched per node on demand.
 */
export function filterNodeTraces(
	traces: NodeTrace[],
	status: NodeTraceStatusFilter | string,
	search: string,
): NodeTrace[] {
	const needle = search.trim().toLowerCase();
	return traces.filter((trace) => {
		const normalized = trace.status.trim().toLowerCase();
		if (status === 'failed' && !isFailedNodeTrace(trace)) return false;
		if (status !== 'all' && status !== 'failed' && normalized !== status) {
			return false;
		}
		if (!needle) return true;
		return (
			trace.nodeId.toLowerCase().includes(needle) ||
			trace.nodeName.toLowerCase().includes(needle) ||
			trace.nodeType.toLowerCase().includes(needle)
		);
	});
}

/** Retry and failure totals rendered above the node list. */
export function summarizeNodeTraces(traces: NodeTrace[]): {
	total: number;
	failed: number;
	retries: number;
} {
	return {
		total: traces.length,
		failed: traces.filter(isFailedNodeTrace).length,
		retries: traces.reduce((sum, trace) => sum + trace.retryCount, 0),
	};
}
