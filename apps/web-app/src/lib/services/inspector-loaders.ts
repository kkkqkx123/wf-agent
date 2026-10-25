import type {
	ExecutionHierarchy,
	ExecutionSubtree,
	NodeExecution,
	NodeTrace,
} from '#lib/types/models';
import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
import {
	getExecutionCriticalPath,
	getExecutionDecisionPoints,
	getExecutionEfficiency,
	getExecutionFailedNodes,
	getExecutionGraphOverview,
	getExecutionSlowNodes,
	type EfficiencyEntry,
	type SlowNodeEntry,
} from '#lib/services/graph';
import {
	getExecutionHierarchy,
	getExecutionHistory,
	getExecutionSubtree,
} from '#lib/services/executions';
import { getExecutionNodeTraces } from '#lib/services/node-trace';

/** Graph tab snapshot: display nodes/edges plus failure overlays. */
export interface GraphSnapshot {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
	failedNodes: string[];
	criticalPath: string[];
}

export async function loadGraphData(id: string): Promise<GraphSnapshot> {
	const overview = await getExecutionGraphOverview(id);
	return {
		nodes: overview.graph.nodes.map((node) => ({
			id: node.id,
			label: node.label,
			kind: node.kind,
			status: node.status,
		})),
		edges: overview.graph.edges.map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			label: edge.label,
			kind: edge.kind,
		})),
		failedNodes: overview.failedNodes,
		criticalPath: overview.criticalPath,
	};
}

/** Analysis tab snapshot: every slow/decision/failure signal at once. */
export interface AnalysisSnapshot {
	slowNodes: SlowNodeEntry[];
	decisionPoints: string[];
	failedNodes: string[];
	criticalPath: string[];
	efficiency: EfficiencyEntry | null;
}

export async function loadAnalysisData(id: string): Promise<AnalysisSnapshot> {
	const [slow, points, failed, critical, ratio] = await Promise.all([
		getExecutionSlowNodes(id),
		getExecutionDecisionPoints(id),
		getExecutionFailedNodes(id),
		getExecutionCriticalPath(id),
		getExecutionEfficiency(id),
	]);
	return {
		slowNodes: slow,
		decisionPoints: points,
		failedNodes: failed,
		criticalPath: critical,
		efficiency: ratio,
	};
}

/** State tab snapshot from the unified history view. */
export interface StateSnapshot {
	variables: Array<{ key: string; value: string }>;
	nodes: NodeExecution[];
}

export async function loadStateData(id: string): Promise<StateSnapshot> {
	const history = await getExecutionHistory(id, ['variables', 'nodes']);
	return {
		variables: history.variables,
		nodes: history.nodes,
	};
}

/** Trace tab snapshot: per-node records plus the skip count. */
export interface TraceSnapshot {
	items: NodeTrace[];
	skipped: number;
}

export async function loadTraceData(id: string): Promise<TraceSnapshot> {
	const page = await getExecutionNodeTraces(id);
	return { items: page.items, skipped: page.skipped };
}

/** Hierarchy tab snapshot: position plus the full subtree. */
export interface HierarchySnapshot {
	hierarchy: ExecutionHierarchy;
	subtree: ExecutionSubtree;
}

export async function loadHierarchyData(id: string): Promise<HierarchySnapshot> {
	const [view, tree] = await Promise.all([getExecutionHierarchy(id), getExecutionSubtree(id)]);
	return { hierarchy: view, subtree: tree };
}
