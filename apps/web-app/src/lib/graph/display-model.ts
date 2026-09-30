/**
 * Display model for the three graph views (workflow definition, agent
 * decision, execution). Single source of truth for node shapes, edge line
 * styles, status colors and legend copy. The backend only supplies pure
 * topology; coordinates and visual encoding always live here.
 */

import {
	nodePorts,
	parseStaticNodeType,
	type StaticNodeType,
} from './node-kind';

export type GraphPreset = 'workflow' | 'decision' | 'execution';

export type GraphLayoutKind = 'layered' | 'columns' | 'force' | 'grid';

export interface DisplayNode {
	id: string;
	label: string;
	kind: string;
	status?: string;
	iteration?: number;
	/** Frontend-only group membership; empty means ungrouped. */
	groupId?: string;
	/** Human label for the group; falls back to the group id. */
	groupLabel?: string;
}

export interface DisplayEdge {
	id: string;
	source: string;
	target: string;
	label?: string;
	kind?: string;
	status?: string;
	taken?: boolean;
}

/**
 * What a node does, independent of backend naming variants. The renderer
 * dispatches on this instead of branching on raw kind strings, so a new node
 * family only has to add one entry to the sets below.
 *
 * The set members are backend `StaticNodeType` names and nothing else: any
 * literal that the enum does not define would encode a contract the backend
 * cannot honour.
 */
export type NodeRenderKind = 'terminal' | 'decision' | 'tool' | 'step';

/** Graph entries and exits; drawn as ellipses. */
const TERMINAL_TYPES = new Set<StaticNodeType>([
	'START',
	'END',
	'START_FROM_MESSAGE',
	'CONTINUE_FROM_MESSAGE',
]);

/** Types with more than one outgoing branch; drawn as diamonds. */
const DECISION_TYPES = new Set<StaticNodeType>(['ROUTE']);

/** Model-invoking types; drawn as hexagons outside the workflow preset. */
const TOOL_TYPES = new Set<StaticNodeType>(['LLM']);

/**
 * Canonical render role for a backend node kind.
 *
 * Plugin-contributed types are not in `StaticNodeType`, so they have no entry
 * here and render as ordinary steps; the same holds for kinds that cannot be
 * parsed at all, which keeps a graph drawable instead of failing to render.
 */
export function renderKind(kind: string): NodeRenderKind {
	const type = parseStaticNodeType(kind);
	if (type === null) return 'step';
	if (TERMINAL_TYPES.has(type)) return 'terminal';
	if (DECISION_TYPES.has(type)) return 'decision';
	if (TOOL_TYPES.has(type)) return 'tool';
	return 'step';
}

export interface LegendEntry {
	label: string;
	shape:
		| 'ellipse'
		| 'diamond'
		| 'rounded'
		| 'hexagon'
		| 'line-solid'
		| 'line-dashed';
	color: string;
}

/** Cytoscape shape name for a node kind within a preset. */
export function nodeShape(kind: string, preset: GraphPreset): string {
	switch (renderKind(kind)) {
		case 'terminal':
			return 'ellipse';
		case 'decision':
			return 'diamond';
		case 'tool':
			return preset === 'workflow' ? 'round-rectangle' : 'hexagon';
		default:
			return 'round-rectangle';
	}
}

/** Backend edge type for a display edge kind; conditional and error routes survive round-trips. */
export function backendEdgeType(kind: string | undefined): string {
	const normalized = (kind ?? '').trim().toLowerCase();
	if (
		normalized === 'conditional' ||
		normalized === 'condition' ||
		normalized === 'branch'
	) {
		return 'CONDITIONAL';
	}
	if (normalized === 'error' || normalized === 'error_route') return 'ERROR';
	return 'DEFAULT';
}

/** Whether an edge renders dashed (conditional, error-route, untaken). */
export function isDashedEdge(
	kind: string | undefined,
	taken: boolean = true,
): boolean {
	if (!taken) return true;
	const normalized = (kind ?? '').trim().toLowerCase();
	return (
		normalized === 'conditional' ||
		normalized === 'condition' ||
		normalized === 'error' ||
		normalized === 'error_route' ||
		normalized === 'branch'
	);
}

const TONE_HEX: Record<string, string> = {
	success: '#16a34a',
	danger: '#dc2626',
	running: '#2563eb',
	warning: '#d97706',
	info: '#0284c7',
	neutral: '#71717a',
};

/** Hex color for a status value; unknown statuses map to neutral. */
export function statusHex(status: string | null | undefined): string {
	if (!status) return TONE_HEX.neutral;
	const normalized = status.trim().toLowerCase();
	if (normalized === 'cached' || normalized === 'info') return TONE_HEX.info;
	const tone = toneForStatus(status);
	switch (tone) {
		case 'success':
			return TONE_HEX.success;
		case 'error':
			return TONE_HEX.danger;
		case 'running':
			return TONE_HEX.running;
		case 'warning':
			return TONE_HEX.warning;
		default:
			return TONE_HEX.neutral;
	}
}

/** Legend entries for a preset. */
export function legendFor(preset: GraphPreset): LegendEntry[] {
	const base: LegendEntry[] = [
		{ label: 'Terminal', shape: 'ellipse', color: TONE_HEX.neutral },
		{ label: 'Step', shape: 'rounded', color: TONE_HEX.neutral },
		{ label: 'Tool / LLM', shape: 'hexagon', color: TONE_HEX.info },
		{ label: 'Flow', shape: 'line-solid', color: TONE_HEX.neutral },
		{ label: 'Conditional', shape: 'line-dashed', color: TONE_HEX.warning },
	];
	if (preset === 'decision') {
		base.push({ label: 'Error', shape: 'diamond', color: TONE_HEX.danger });
	}
	return base;
}

/** Distinct node kinds present, for the filter panel. */
export function distinctKinds(nodes: DisplayNode[]): string[] {
	return [...new Set(nodes.map((node) => node.kind || 'unknown'))].sort();
}

/** Per-kind node counts, for the large-graph aggregation notice. */
export function kindCounts(
	nodes: DisplayNode[],
): Array<{ kind: string; count: number }> {
	const counts = new Map<string, number>();
	for (const node of nodes) {
		const kind = node.kind || 'unknown';
		counts.set(kind, (counts.get(kind) ?? 0) + 1);
	}
	return [...counts.entries()]
		.map(([kind, count]) => ({ kind, count }))
		.sort((a, b) => b.count - a.count);
}

/** Maximum nodes rendered; larger graphs show a truncation notice. */
export const GRAPH_NODE_CAP = 800;

/**
 * Per-preset edge label budget. Denser execution graphs hide sooner so
 * labels stay readable instead of collapsing into noise.
 */
export const EDGE_LABEL_LIMIT: Record<GraphPreset, number> = {
	workflow: 60,
	decision: 60,
	execution: 40,
};

/**
 * Importance score for an edge label. Semantic edges (error routes,
 * conditionals, branches) outrank plain flow edges; short labels outrank
 * long ones. Unlabeled edges score far below zero so they never take a
 * label slot.
 */
export function scoreEdgeLabel(edge: DisplayEdge): number {
	const label = (edge.label ?? '').trim();
	if (!label) return -100;
	const kind = (edge.kind ?? '').trim().toLowerCase();
	let score = Math.max(0, 12 - label.length);
	if (kind === 'error' || kind === 'error_route') {
		score += 8;
	} else if (
		kind === 'conditional' ||
		kind === 'condition' ||
		kind === 'branch'
	) {
		score += 5;
	}
	return score;
}

export interface CappedGraph {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
	truncated: boolean;
	total: number;
}

/**
 * Enforce the node cap with round-robin sampling across kinds, so a large
 * graph keeps every kind represented instead of cutting off the tail.
 * Retained ids (failed, running, critical path, selection) are kept first
 * in the given order; leftovers fill the remaining budget by sampling.
 * Edges survive only when both endpoints survive.
 */
export function capGraph(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	cap: number = GRAPH_NODE_CAP,
	retainIds: Iterable<string> = [],
): CappedGraph {
	if (nodes.length <= cap) {
		return { nodes, edges, truncated: false, total: nodes.length };
	}
	const byId = new Map(nodes.map((node) => [node.id, node]));
	const retained: DisplayNode[] = [];
	const retainedIds = new Set<string>();
	for (const id of new Set(retainIds)) {
		if (retained.length >= cap) break;
		const node = byId.get(id);
		if (node && !retainedIds.has(id)) {
			retained.push(node);
			retainedIds.add(id);
		}
	}
	if (retained.length >= cap) {
		const keptIds = new Set(retained.map((node) => node.id));
		return {
			nodes: retained,
			edges: edges.filter(
				(edge) => keptIds.has(edge.source) && keptIds.has(edge.target),
			),
			truncated: true,
			total: nodes.length,
		};
	}
	const rest = nodes.filter((node) => !retainedIds.has(node.id));
	const buckets = new Map<string, DisplayNode[]>();
	for (const node of rest) {
		const kind = node.kind || 'unknown';
		const bucket = buckets.get(kind) ?? [];
		bucket.push(node);
		buckets.set(kind, bucket);
	}
	const kept: DisplayNode[] = [...retained];
	const kinds = [...buckets.keys()];
	let round = 0;
	let progressed = true;
	while (kept.length < cap && progressed) {
		progressed = false;
		for (const kind of kinds) {
			const bucket = buckets.get(kind);
			if (bucket && round < bucket.length && kept.length < cap) {
				kept.push(bucket[round]);
				progressed = true;
			}
		}
		round += 1;
	}
	const keptIds = new Set(kept.map((node) => node.id));
	return {
		nodes: kept,
		edges: edges.filter(
			(edge) => keptIds.has(edge.source) && keptIds.has(edge.target),
		),
		truncated: true,
		total: nodes.length,
	};
}

/** Short label for canvas rendering; long names truncate with ellipsis. */

/** Short label for canvas rendering; long names truncate with ellipsis. */
export function shortLabel(label: string, max: number = 18): string {
	const trimmed = (label ?? '').trim() || 'unnamed';
	return trimmed.length > max ? `${trimmed.slice(0, max - 1)}…` : trimmed;
}

/**
 * Canonical execution tone. When several signals apply to one node the
 * highest-ranked tone wins, so colors never depend on update order.
 */
export type ExecutionTone =
	'running' | 'error' | 'warning' | 'success' | 'neutral';

const TONE_RANK: Record<ExecutionTone, number> = {
	running: 4,
	error: 3,
	warning: 2,
	success: 1,
	neutral: 0,
};

/** Normalize any backend status string to a canonical execution tone. */
export function toneForStatus(
	status: string | null | undefined,
): ExecutionTone {
	if (!status) return 'neutral';
	const normalized = status.trim().toLowerCase();
	if (
		['running', 'in_progress', 'executing', 'streaming', 'started'].includes(
			normalized,
		)
	) {
		return 'running';
	}
	if (
		['failed', 'failure', 'error', 'errored', 'timeout', 'aborted'].includes(
			normalized,
		)
	) {
		return 'error';
	}
	if (
		['paused', 'pending', 'queued', 'waiting', 'retrying', 'cached'].includes(
			normalized,
		)
	) {
		return 'warning';
	}
	if (
		[
			'completed',
			'complete',
			'success',
			'succeeded',
			'done',
			'ok',
			'active',
			'enabled',
		].includes(normalized)
	) {
		return 'success';
	}
	return 'neutral';
}

/** Higher-ranked tone wins; used to merge execution, validation and edit signals. */
export function rankTone(
	current: ExecutionTone,
	next: ExecutionTone,
): ExecutionTone {
	return TONE_RANK[next] > TONE_RANK[current] ? next : current;
}

/** Status value the canvas understands for a canonical tone. */
export function statusForTone(tone: ExecutionTone): string | undefined {
	switch (tone) {
		case 'running':
			return 'running';
		case 'error':
			return 'failed';
		case 'warning':
			return 'pending';
		case 'success':
			return 'completed';
		default:
			return undefined;
	}
}

export interface ConnectByPortCheck {
	sourceKind: string;
	targetKind: string;
}

/**
 * Reject a connection whose endpoints break the backend boundary rules:
 * entry nodes accept no incoming edge and exit nodes emit none. The reason
 * strings match the backend graph validator so a canvas refusal reads the
 * same as the server-side draft error.
 */
export function connectByPort(check: ConnectByPortCheck): string | null {
	const sourceType = parseStaticNodeType(check.sourceKind);
	const targetType = parseStaticNodeType(check.targetKind);
	if (sourceType !== null && !nodePorts(sourceType).emitsOutput) {
		return `${sourceType} node cannot have outgoing edges`;
	}
	if (targetType !== null && !nodePorts(targetType).acceptsInput) {
		return `${targetType} node cannot have incoming edges`;
	}
	return null;
}
