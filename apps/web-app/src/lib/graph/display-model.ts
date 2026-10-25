/**
 * Display model for the three graph views (workflow definition, agent
 * decision, execution). Single source of truth for node shapes, edge line
 * styles, status colors and legend copy. The backend only supplies pure
 * topology; coordinates and visual encoding always live here.
 */

import { browser } from '$app/env';
import {
	nodePorts,
	parseStaticNodeType,
	type StaticNodeType,
} from './node-kind';

export type GraphPreset = 'workflow' | 'decision' | 'execution';

export type GraphLayoutKind = 'layered' | 'columns' | 'rows' | 'compact';

/** A labeled set of node ids highlighted together, e.g. an analysis result. */
export interface GraphOverlay {
	id: string;
	label: string;
	ids: string[];
}

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
	if (isFailureBranch({ kind })) return true;
	const normalized = (kind ?? '').trim().toLowerCase();
	return (
		normalized === 'conditional' ||
		normalized === 'condition' ||
		normalized === 'error' ||
		normalized === 'error_route' ||
		normalized === 'branch'
	);
}

/** Whether an edge is a failure branch: error routes and explicit fail labels. */
export function isFailureBranch(edge: {
	kind?: string;
	label?: string;
}): boolean {
	const kind = (edge.kind ?? '').trim().toLowerCase();
	if (kind === 'error' || kind === 'error_route') return true;
	const label = (edge.label ?? '').trim().toLowerCase();
	return (
		label === 'fail' ||
		label === 'failed' ||
		label === 'failure' ||
		label === 'error' ||
		label === 'false' ||
		label === 'no'
	);
}

/** Whether an edge status reads as waiting and should fade. */
export function isWaitingStatus(status: string | null | undefined): boolean {
	if (!status) return false;
	const normalized = status.trim().toLowerCase();
	return (
		normalized === 'pending' ||
		normalized === 'queued' ||
		normalized === 'waiting' ||
		normalized === 'paused' ||
		normalized === 'suspended'
	);
}

export interface EdgeGradient {
	from: string;
	to: string;
	waiting: boolean;
}

/** Source-target gradient colors for one edge; null when no blend applies. */
export function edgeGradient(
	edge: DisplayEdge,
	statusById: Map<string, string | undefined>,
): EdgeGradient | null {
	const fromStatus = statusById.get(edge.source);
	const toStatus = statusById.get(edge.target);
	if (!fromStatus && !toStatus) return null;
	const from = resolveStatusHex(fromStatus ?? edge.status);
	const to = resolveStatusHex(toStatus ?? edge.status);
	if (from === to) return null;
	return {
		from,
		to,
		waiting: isWaitingStatus(fromStatus) || isWaitingStatus(toStatus),
	};
}

const TONE_FALLBACK: Record<string, string> = {
	success: 'hsl(152 55% 38%)',
	danger: 'hsl(0 72% 48%)',
	running: 'hsl(252 56% 54%)',
	warning: 'hsl(32 88% 44%)',
	info: 'hsl(210 80% 48%)',
	neutral: 'hsl(215 12% 46%)',
};

/** Hex-free color for a status value; unknown statuses map to neutral. */
export function statusHex(status: string | null | undefined): string {
	if (!status) return TONE_FALLBACK.neutral;
	const normalized = status.trim().toLowerCase();
	if (normalized === 'cached' || normalized === 'info')
		return TONE_FALLBACK.info;
	const tone = toneForStatus(status);
	switch (tone) {
		case 'success':
			return TONE_FALLBACK.success;
		case 'error':
			return TONE_FALLBACK.danger;
		case 'running':
			return TONE_FALLBACK.running;
		case 'warning':
			return TONE_FALLBACK.warning;
		default:
			return TONE_FALLBACK.neutral;
	}
}

/** Legend entries for a preset. */
export function legendFor(preset: GraphPreset): LegendEntry[] {
	const tone = (name: string, fallback: string): string => {
		if (!browser || typeof document === 'undefined') return fallback;
		const raw = getComputedStyle(document.documentElement)
			.getPropertyValue(name)
			.trim();
		return raw ? `hsl(${raw})` : fallback;
	};
	const neutral = tone('--muted-foreground', TONE_FALLBACK.neutral);
	const base: LegendEntry[] = [
		{ label: 'Terminal', shape: 'ellipse', color: neutral },
		{ label: 'Step', shape: 'rounded', color: neutral },
		{
			label: 'Tool / LLM',
			shape: 'hexagon',
			color: tone('--info', TONE_FALLBACK.info),
		},
		{ label: 'Flow', shape: 'line-solid', color: neutral },
		{
			label: 'Conditional',
			shape: 'line-dashed',
			color: tone('--warning', TONE_FALLBACK.warning),
		},
	];
	if (preset === 'decision') {
		base.push({
			label: 'Error',
			shape: 'diamond',
			color: tone('--destructive', TONE_FALLBACK.danger),
		});
	}
	return base;
}

/**
 * Theme-aware status color for canvas element data. Reads the computed
 * semantic tokens in the browser and falls back to the static hex map
 * during server rendering and tests.
 */
export function resolveStatusHex(status: string | null | undefined): string {
	if (!browser || typeof document === 'undefined') return statusHex(status);
	const read = (name: string, fallback: string): string => {
		const raw = getComputedStyle(document.documentElement)
			.getPropertyValue(name)
			.trim();
		return raw ? `hsl(${raw})` : fallback;
	};
	if (!status) return read('--muted-foreground', TONE_FALLBACK.neutral);
	const normalized = status.trim().toLowerCase();
	if (normalized === 'cached' || normalized === 'info')
		return read('--info', TONE_FALLBACK.info);
	switch (toneForStatus(status)) {
		case 'success':
			return read('--success', TONE_FALLBACK.success);
		case 'error':
			return read('--destructive', TONE_FALLBACK.danger);
		case 'running':
			return read('--running', TONE_FALLBACK.running);
		case 'warning':
			return read('--warning', TONE_FALLBACK.warning);
		default:
			return read('--muted-foreground', TONE_FALLBACK.neutral);
	}
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
		[
			'running',
			'in_progress',
			'executing',
			'streaming',
			'started',
			'resumed',
		].includes(normalized)
	) {
		return 'running';
	}
	if (
		[
			'failed',
			'failure',
			'error',
			'errored',
			'timeout',
			'timed_out',
			'aborted',
			'crashed',
			'rejected',
			'unhealthy',
		].includes(normalized)
	) {
		return 'error';
	}
	if (
		[
			'paused',
			'pending',
			'queued',
			'waiting',
			'suspended',
			'retrying',
			'degraded',
			'cached',
		].includes(normalized)
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
			'healthy',
			'active',
			'enabled',
			'approved',
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
	/** Hotspot port the drag started from; input ports never start a link. */
	sourcePort?: string | null;
	/** Hotspot port the drag lands on; output ports never accept a link. */
	targetPort?: string | null;
}

/** Coarse port role of a node kind for connection validation. */
export type PortRole = 'entry' | 'exit' | 'decision' | 'middle';

/**
 * Port role of a backend node kind: entries accept no input, exits emit no
 * output, decisions expose a branch port, everything else is a middle step.
 * Unknown kinds are ordinary middle nodes so the canvas stays drawable.
 */
export function portType(kind: string): PortRole {
	const ports = nodePorts(kind);
	if (!ports.acceptsInput) return 'entry';
	if (!ports.emitsOutput) return 'exit';
	return renderKind(kind) === 'decision' ? 'decision' : 'middle';
}

/**
 * Reject a connection whose endpoints break the backend boundary rules:
 * entry nodes accept no incoming edge and exit nodes emit none. Port
 * direction is validated first: links start at an output or branch port
 * and land on an input port, mirroring the hotspot model in GraphCanvas.
 * The reason strings match the backend graph validator so a canvas refusal
 * reads the same as the server-side draft error.
 */
export function connectByPort(check: ConnectByPortCheck): string | null {
	if (check.sourcePort === 'input') {
		return 'Connections start from an output port';
	}
	if (check.targetPort === 'output' || check.targetPort === 'branch') {
		return 'Connections land on an input port';
	}
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
