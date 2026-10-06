import { client } from '$lib/api/client';
import { call, requireData } from '$lib/api/envelope';
import type { components } from '$lib/api/schema';
import type {
	CycleResult,
	DecisionPathStep,
	ExecutionPathStat,
	GraphAnalysisResult,
	GraphSummary,
	PromoteReport,
	ReachabilityResult,
	TopologyResult,
	ValidationIssue,
	WorkflowGraph,
} from '$lib/types/models';

type NodeDoc = components['schemas']['GraphNodeDoc'];
type EdgeDoc = components['schemas']['GraphEdgeDoc'];
type DecisionNodeDoc = components['schemas']['DecisionNodeDoc'];
type DecisionEdgeDoc = components['schemas']['DecisionEdgeDoc'];
type DecisionGraphDoc = components['schemas']['DecisionGraphDoc'];
type StepDoc = components['schemas']['ExecutionPathStepDoc'];

/** Topology-only graph; layout is always computed by the canvas. */
export function toWorkflowGraph(
	nodes: NodeDoc[],
	edges: EdgeDoc[],
	statusById?: Map<string, string>,
): WorkflowGraph {
	return {
		nodes: nodes.map((node) => ({
			id: node.id,
			label: node.name ?? node.id,
			kind: node.node_type,
			status: statusById?.get(node.id),
		})),
		edges: edges.map((edge) => ({
			id: edge.id,
			from: edge.source_node_id,
			to: edge.target_node_id,
			label: edge.condition ?? undefined,
			kind: (edge.edge_type ?? 'DEFAULT').toLowerCase(),
		})),
	};
}

/** Decision graph mapped onto the shared topology model. */
export function toDecisionGraph(
	view: DecisionGraphDoc,
	statusById?: Map<string, string>,
): WorkflowGraph {
	const errorIds = new Set(view.error_node_ids ?? []);
	return {
		nodes: (view.nodes ?? []).map((node) => ({
			id: node.node_id,
			label: node.description || node.node_id,
			kind: node.type,
			status:
				statusById?.get(node.node_id) ??
				(errorIds.has(node.node_id) ? 'failed' : undefined),
			iteration: node.iteration,
		})),
		edges: (view.edges ?? []).map((edge) => ({
			id: edge.edge_id,
			from: edge.from_node_id,
			to: edge.to_node_id,
			label: edge.reason ?? edge.condition ?? undefined,
			taken: edge.was_taken ?? true,
		})),
	};
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

/** Workflow definition graph endpoints (typed view layer). */
export async function getGraphNodes(
	id: string,
	nodeType?: string,
): Promise<NodeDoc[]> {
	const data = await call<NodeDoc[] | null>(
		client.GET('/api/v1/workflows/{id}/graph/nodes', {
			params: { path: { id }, query: nodeType ? { node_type: nodeType } : {} },
		}),
	);
	return requireData(data, `Graph nodes missing for workflow ${id}`);
}

export async function getGraphEdges(id: string): Promise<EdgeDoc[]> {
	const data = await call<EdgeDoc[] | null>(
		client.GET('/api/v1/workflows/{id}/graph/edges', {
			params: { path: { id } },
		}),
	);
	return requireData(data, `Graph edges missing for workflow ${id}`);
}

export async function getGraphSummary(id: string): Promise<GraphSummary> {
	const data = await call<components['schemas']['GraphSummaryDoc'] | null>(
		client.GET('/api/v1/workflows/{id}/graph/summary', {
			params: { path: { id } },
		}),
	);
	if (!data) throw new Error(`Graph summary missing for workflow ${id}`);
	return {
		workflowId: data.workflow_id,
		nodeCount: data.node_count,
		edgeCount: data.edge_count,
		startNodeId: data.start_node_id ?? null,
		endNodeIds: data.end_node_ids ?? [],
		nodeCountsByType: data.node_counts_by_type ?? {},
	};
}

export async function getGraphNeighbors(
	id: string,
	nodeId: string,
): Promise<{ predecessors: string[]; successors: string[] }> {
	const data = await call<components['schemas']['GraphNeighborsDoc'] | null>(
		client.GET('/api/v1/workflows/{id}/graph/neighbors/{nodeId}', {
			params: { path: { id, nodeId } },
		}),
	);
	const neighbors = requireData(
		data,
		`Graph neighbors missing for node ${nodeId}`,
	);
	return {
		predecessors: neighbors.predecessors ?? [],
		successors: neighbors.successors ?? [],
	};
}

export async function getGraphAnalysis(
	id: string,
): Promise<GraphAnalysisResult> {
	const data = await call<components['schemas']['GraphAnalysisDoc'] | null>(
		client.GET('/api/v1/workflows/{id}/graph/analysis', {
			params: { path: { id } },
		}),
	);
	if (!data) throw new Error(`Graph analysis missing for workflow ${id}`);
	return {
		cycleDetection: {
			hasCycle: data.cycle_detection.has_cycle,
			cycleNodes: data.cycle_detection.cycle_nodes ?? [],
			cycleEdges: data.cycle_detection.cycle_edges ?? [],
		},
		topologicalSort: {
			success: data.topological_sort.success,
			sortedNodes: data.topological_sort.sorted_nodes ?? [],
			cycleNodes: data.topological_sort.cycle_nodes ?? [],
		},
		reachability: {
			reachableFromStart: data.reachability.reachable_from_start ?? [],
			reachableToEnd: data.reachability.reachable_to_end ?? [],
			unreachableNodes: data.reachability.unreachable_nodes ?? [],
			deadEndNodes: data.reachability.dead_end_nodes ?? [],
		},
		nodeTotal: data.node_total,
		edgeTotal: data.edge_total,
		nodeCountsByType: data.node_counts_by_type ?? {},
	};
}

export async function getGraphCycles(id: string): Promise<CycleResult> {
	const data = await call<components['schemas']['CycleDetectionDoc'] | null>(
		client.GET('/api/v1/workflows/{id}/graph/cycles', {
			params: { path: { id } },
		}),
	);
	const cycles = requireData(data, `Cycle report missing for workflow ${id}`);
	return {
		hasCycle: cycles.has_cycle ?? false,
		cycleNodes: cycles.cycle_nodes ?? [],
		cycleEdges: cycles.cycle_edges ?? [],
	};
}

export async function getGraphTopology(id: string): Promise<TopologyResult> {
	const data = await call<components['schemas']['TopologicalSortDoc'] | null>(
		client.GET('/api/v1/workflows/{id}/graph/topology', {
			params: { path: { id } },
		}),
	);
	const sort = requireData(data, `Topology report missing for workflow ${id}`);
	return {
		success: sort.success ?? false,
		sortedNodes: sort.sorted_nodes ?? [],
		cycleNodes: sort.cycle_nodes ?? [],
	};
}

export async function getGraphReachability(
	id: string,
): Promise<ReachabilityResult> {
	const data = await call<components['schemas']['ReachabilityDoc'] | null>(
		client.GET('/api/v1/workflows/{id}/graph/reachability', {
			params: { path: { id } },
		}),
	);
	const reachability = requireData(
		data,
		`Reachability report missing for workflow ${id}`,
	);
	return {
		reachableFromStart: reachability.reachable_from_start ?? [],
		reachableToEnd: reachability.reachable_to_end ?? [],
		unreachableNodes: reachability.unreachable_nodes ?? [],
		deadEndNodes: reachability.dead_end_nodes ?? [],
	};
}

/** Execution graph endpoints (same view layer, execution scope). */
export async function getExecutionGraphNodes(
	executionId: string,
): Promise<NodeDoc[]> {
	const data = await call<NodeDoc[] | null>(
		client.GET('/api/v1/executions/{id}/graph/nodes', {
			params: { path: { id: executionId } },
		}),
	);
	return requireData(data, `Graph nodes missing for execution ${executionId}`);
}

export async function getExecutionGraphEdges(
	executionId: string,
): Promise<EdgeDoc[]> {
	const data = await call<EdgeDoc[] | null>(
		client.GET('/api/v1/executions/{id}/graph/edges', {
			params: { path: { id: executionId } },
		}),
	);
	return requireData(data, `Graph edges missing for execution ${executionId}`);
}

export async function getExecutionGraphNeighbors(
	executionId: string,
	nodeId: string,
): Promise<{ predecessors: string[]; successors: string[] }> {
	const data = await call<components['schemas']['GraphNeighborsDoc'] | null>(
		client.GET('/api/v1/executions/{id}/graph/neighbors/{nodeId}', {
			params: { path: { id: executionId, nodeId } },
		}),
	);
	const neighbors = requireData(
		data,
		`Graph neighbors missing for node ${nodeId}`,
	);
	return {
		predecessors: neighbors.predecessors ?? [],
		successors: neighbors.successors ?? [],
	};
}

export async function getExecutionPathStats(
	executionId: string,
): Promise<ExecutionPathStat[]> {
	const data = await call<
		components['schemas']['ExecutionPathStatsDoc'][] | null
	>(
		client.GET('/api/v1/executions/{id}/graph/path-stats', {
			params: { path: { id: executionId } },
		}),
	);
	const stats = requireData(
		data,
		`Path stats missing for execution ${executionId}`,
	);
	return stats.map((stat) => ({
		nodeCount: stat.node_count,
		edgeCount: stat.edge_count,
		nodes: stat.nodes ?? [],
	}));
}

/** Agent decision graph endpoints (typed view layer). */
export async function getDecisionGraph(id: string): Promise<DecisionGraphDoc> {
	const data = await call<DecisionGraphDoc | null>(
		client.GET('/api/v1/agent-loops/{id}/graph', {
			params: { path: { id } },
		}),
	);
	if (!data) throw new Error(`Decision graph missing for loop ${id}`);
	return data;
}

export async function getDecisionNodes(id: string): Promise<DecisionNodeDoc[]> {
	const data = await call<DecisionNodeDoc[] | null>(
		client.GET('/api/v1/agent-loops/{id}/graph/nodes', {
			params: { path: { id } },
		}),
	);
	return requireData(data, `Decision nodes missing for loop ${id}`);
}

export async function getDecisionEdges(id: string): Promise<DecisionEdgeDoc[]> {
	const data = await call<DecisionEdgeDoc[] | null>(
		client.GET('/api/v1/agent-loops/{id}/graph/edges', {
			params: { path: { id } },
		}),
	);
	return requireData(data, `Decision edges missing for loop ${id}`);
}

export async function getDecisionSteps(
	id: string,
): Promise<DecisionPathStep[]> {
	const data = await call<StepDoc[] | null>(
		client.GET('/api/v1/agent-loops/{id}/graph/paths/steps', {
			params: { path: { id } },
		}),
	);
	const steps = requireData(data, `Decision steps missing for loop ${id}`);
	return steps.map((step) => ({
		stepNo: step.step_no,
		nodeId: step.node_id,
		nodeType: step.node_type,
		description: step.description,
		iteration: step.iteration,
		timestamp: step.timestamp,
		duration: step.duration ?? null,
	}));
}

/**
 * Tool-call frequency answers with a plain `{ tool: count }` map (still
 * free-form in the contract); narrow it structurally here.
 */
export async function getDecisionToolFrequency(
	id: string,
): Promise<Array<{ tool: string; count: number }>> {
	const data = await call<unknown>(
		client.GET('/api/v1/agent-loops/{id}/graph/tool-frequency', {
			params: { path: { id } },
		}),
	);
	requireData(data, `Tool frequency missing for loop ${id}`);
	if (!isRecord(data)) return [];
	return Object.entries(data)
		.filter((entry): entry is [string, number] => typeof entry[1] === 'number')
		.map(([tool, count]) => ({ tool, count }))
		.sort((a, b) => b.count - a.count);
}

/**
 * Save a full workflow definition as an editable draft. The backend
 * overwrites the stored document and echoes the draft id, so callers must
 * pass a complete definition, never a partial patch.
 */
export async function saveWorkflowDraft(
	definition: Record<string, unknown>,
): Promise<string> {
	const data = await call<unknown>(
		client.POST('/api/v1/workflows/drafts', { body: definition }),
	);
	const id = requireData(data, 'Draft save returned no id');
	return String(id);
}

/** Draft validation issues (typed). */
export async function validateWorkflowDraft(
	id: string,
): Promise<ValidationIssue[]> {
	const data = await call<components['schemas']['ValidationIssueDoc'][] | null>(
		client.GET('/api/v1/workflows/drafts/{id}/validate', {
			params: { path: { id } },
		}),
	);
	const issues = requireData(data, `Draft validation missing for ${id}`);
	return issues.map((issue) => ({
		field: issue.field,
		message: issue.message,
	}));
}

/** Delete a workflow draft used only for a validation check. */
export async function deleteWorkflowDraft(id: string): Promise<void> {
	await call<unknown>(
		client.DELETE('/api/v1/workflows/drafts/{id}', {
			params: { path: { id } },
		}),
	);
}

/** Draft promotion report (typed). */
export async function promoteWorkflowDraft(id: string): Promise<PromoteReport> {
	const data = await call<components['schemas']['PromoteReportDoc'] | null>(
		client.POST('/api/v1/workflows/drafts/{id}/promote', {
			params: { path: { id } },
		}),
	);
	if (!data) throw new Error(`Promotion report missing for draft ${id}`);
	return {
		resourceKind: data.resource_kind,
		resourceId: data.resource_id,
		dependents: (data.dependents ?? []).map((entry) => ({
			workflowId: entry.workflow_id,
			workflowName: entry.workflow_name,
			nodeId: entry.node_id,
			field: entry.field,
			level: entry.level,
			errors: entry.errors ?? [],
			warnings: entry.warnings ?? [],
		})),
		errorCount: data.error_count,
		warningCount: data.warning_count,
		passCount: data.pass_count,
	};
}

/** Roll back a workflow to a named version. */
export async function rollbackWorkflow(
	id: string,
	version: string,
): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/workflows/{id}/rollback', {
			params: { path: { id } },
			body: { version },
		}),
	);
}

export interface SlowNodeEntry {
	node: string;
	durationMs: number;
	success: boolean;
}

/** Slowest nodes of an execution. */
export async function getExecutionSlowNodes(
	executionId: string,
): Promise<SlowNodeEntry[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/analysis/slow-nodes', {
			params: { path: { id: executionId } },
		}),
	);
	const rows = requireData(
		data,
		`Slow nodes missing for execution ${executionId}`,
	);
	if (!Array.isArray(rows)) return [];
	return rows
		.filter(isRecord)
		.map((row) => ({
			node:
				typeof row.node_id === 'string'
					? row.node_id
					: typeof row.node === 'string'
						? row.node
						: '',
			durationMs:
				typeof row.duration_ms === 'number'
					? row.duration_ms
					: typeof row.durationMs === 'number'
						? row.durationMs
						: 0,
			success: row.success !== false,
		}))
		.filter((entry) => entry.node !== '');
}

/** Branching node ids of an execution. */
export async function getExecutionDecisionPoints(
	executionId: string,
): Promise<string[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/analysis/decision-points', {
			params: { path: { id: executionId } },
		}),
	);
	const rows = requireData(
		data,
		`Decision points missing for execution ${executionId}`,
	);
	if (!Array.isArray(rows)) return [];
	return rows
		.map((row) =>
			isRecord(row) && typeof row.node_id === 'string' ? row.node_id : null,
		)
		.filter((id): id is string => !!id);
}

export interface EfficiencyEntry {
	executedSteps: number;
	optimalSteps: number;
	ratio: number;
	wastefulNodes: number;
	retryCount: number;
}

/** Step efficiency of an execution against the shortest structural path. */
export async function getExecutionEfficiency(
	executionId: string,
): Promise<EfficiencyEntry | null> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/analysis/efficiency', {
			params: { path: { id: executionId } },
		}),
	);
	requireData(data, `Efficiency report missing for execution ${executionId}`);
	if (!isRecord(data)) return null;
	const number = (key: string): number =>
		typeof data[key] === 'number' ? (data[key] as number) : 0;
	return {
		executedSteps: number('executed_steps'),
		optimalSteps: number('optimal_steps'),
		ratio: number('efficiency_ratio'),
		wastefulNodes: number('wasteful_nodes'),
		retryCount: number('retry_count'),
	};
}

/** Failed node ids of an execution. */
export async function getExecutionFailedNodes(
	executionId: string,
): Promise<string[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/failed-nodes', {
			params: { path: { id: executionId } },
		}),
	);
	requireData(data, `Failed nodes missing for execution ${executionId}`);
	if (!Array.isArray(data)) return [];
	return data
		.map((row) => {
			if (typeof row === 'string') return row;
			if (isRecord(row) && typeof row.node_id === 'string') return row.node_id;
			return null;
		})
		.filter((id): id is string => !!id);
}

/** Longest structural path of an execution graph. */
export async function getExecutionCriticalPath(
	executionId: string,
): Promise<string[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/analysis/paths', {
			params: { path: { id: executionId } },
		}),
	);
	requireData(data, `Path analysis missing for execution ${executionId}`);
	if (!isRecord(data)) return [];
	const critical = data.critical_path;
	if (!isRecord(critical) || !Array.isArray(critical.nodes)) return [];
	return critical.nodes.filter(
		(node): node is string => typeof node === 'string',
	);
}

export function graphField(
	record: Record<string, unknown>,
	keys: string[],
	fallback: string,
): string {
	for (const key of keys) {
		const value = record[key];
		if (typeof value === 'string' && value) return value;
	}
	return fallback;
}

export function asRecordList(value: unknown): Record<string, unknown>[] {
	return Array.isArray(value)
		? value.filter(
				(entry): entry is Record<string, unknown> =>
					!!entry && typeof entry === 'object' && !Array.isArray(entry),
			)
		: [];
}

/**
 * Whole execution topology in one round trip. The split nodes/edges
 * endpoints re-resolve the same structure server-side, so prefer this for
 * the graph tab and keep the split calls for filtered (by-type) views.
 */
export async function getExecutionGraph(
	executionId: string,
): Promise<WorkflowGraph> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/graph', {
			params: { path: { id: executionId } },
		}),
	);
	const structure = requireData(
		data,
		`Graph missing for execution ${executionId}`,
	);
	const view = isRecord(structure) ? structure : {};
	return {
		nodes: asRecordList(view.nodes).map((node, index) => ({
			id: graphField(node, ['id', 'node_id'], `node-${index}`),
			label: graphField(
				node,
				['name', 'label', 'id', 'node_id'],
				`node-${index}`,
			),
			kind: graphField(node, ['node_type', 'kind', 'type'], 'unknown'),
		})),
		edges: asRecordList(view.edges).map((edge, index) => ({
			id: graphField(edge, ['id', 'edge_id'], `edge-${index}`),
			from: graphField(edge, ['source_node_id', 'from', 'source'], ''),
			to: graphField(edge, ['target_node_id', 'to', 'target'], ''),
			label:
				typeof edge.condition === 'string' && edge.condition
					? edge.condition
					: undefined,
			kind: graphField(
				edge,
				['edge_type', 'type', 'kind'],
				'default',
			).toLowerCase(),
		})),
	};
}

/**
 * Execution graph overview in one round trip: topology plus failed nodes
 * and critical path. Prefer this for the graph tab; the split endpoints
 * stay for filtered views and the analysis tab.
 */
export async function getExecutionGraphOverview(executionId: string): Promise<{
	graph: WorkflowGraph;
	failedNodes: string[];
	criticalPath: string[];
}> {
	const data = await call<unknown>(
		client.GET('/api/v1/executions/{id}/graph/overview', {
			params: { path: { id: executionId } },
		}),
	);
	const overview = requireData(
		data,
		`Graph overview missing for execution ${executionId}`,
	);
	const view = isRecord(overview) ? overview : {};
	const graphValue = isRecord(view.graph) ? view.graph : {};
	const failedValue = Array.isArray(view.failed_nodes)
		? view.failed_nodes
		: Array.isArray(view.failedNodes)
			? view.failedNodes
			: [];
	const criticalValue = Array.isArray(view.critical_path)
		? view.critical_path
		: Array.isArray(view.criticalPath)
			? view.criticalPath
			: [];
	return {
		graph: {
			nodes: asRecordList(graphValue.nodes).map((node, index) => ({
				id: graphField(node, ['id', 'node_id'], `node-${index}`),
				label: graphField(
					node,
					['name', 'label', 'id', 'node_id'],
					`node-${index}`,
				),
				kind: graphField(node, ['node_type', 'kind', 'type'], 'unknown'),
			})),
			edges: asRecordList(graphValue.edges).map((edge, index) => ({
				id: graphField(edge, ['id', 'edge_id'], `edge-${index}`),
				from: graphField(edge, ['source_node_id', 'from', 'source'], ''),
				to: graphField(edge, ['target_node_id', 'to', 'target'], ''),
				label:
					typeof edge.condition === 'string' && edge.condition
						? edge.condition
						: undefined,
				kind: graphField(
					edge,
					['edge_type', 'type', 'kind'],
					'default',
				).toLowerCase(),
			})),
		},
		failedNodes: failedValue.filter(
			(entry): entry is string => typeof entry === 'string',
		),
		criticalPath: criticalValue.filter(
			(entry): entry is string => typeof entry === 'string',
		),
	};
}

/**
 * Draft payloads wrap the free-form definition in a `definition` envelope
 * on some endpoints and return it bare on others; normalize to the
 * definition record either way.
 */
function unwrapDraftDefinition(record: unknown): Record<string, unknown> {
	if (!isRecord(record)) return {};
	return isRecord(record.definition) ? record.definition : record;
}

/**
 * Read-only topology of a workflow draft. Drafts are free-form and may be
 * incomplete, so every field degrades to a placeholder instead of failing.
 */
export async function getWorkflowDraftTopology(
	draftId: string,
): Promise<WorkflowGraph> {
	const data = await call<unknown>(
		client.GET('/api/v1/workflows/drafts/{id}', {
			params: { path: { id: draftId } },
		}),
	);
	const record = requireData(data, `Draft ${draftId} missing`);
	const definition = unwrapDraftDefinition(record);
	return {
		nodes: asRecordList(definition.nodes).map((node, index) => ({
			id: graphField(node, ['id', 'node_id'], `node-${index}`),
			label: graphField(
				node,
				['name', 'label', 'id', 'node_id'],
				`node-${index}`,
			),
			kind: graphField(node, ['node_type', 'kind', 'type'], 'unknown'),
		})),
		edges: asRecordList(definition.edges).map((edge, index) => ({
			id: graphField(edge, ['id', 'edge_id'], `edge-${index}`),
			from: graphField(edge, ['source_node_id', 'from', 'source'], ''),
			to: graphField(edge, ['target_node_id', 'to', 'target'], ''),
			label:
				typeof edge.condition === 'string' && edge.condition
					? edge.condition
					: undefined,
			kind: graphField(
				edge,
				['edge_type', 'type', 'kind'],
				'default',
			).toLowerCase(),
		})),
	};
}
