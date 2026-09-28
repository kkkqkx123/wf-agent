/**
 * Display model for the three graph views (workflow definition, agent
 * decision, execution). Single source of truth for node shapes, edge line
 * styles, status colors and legend copy. The backend only supplies pure
 * topology; coordinates and visual encoding always live here.
 */

export type GraphPreset = 'workflow' | 'decision' | 'execution';

export type GraphLayoutKind = 'layered' | 'columns' | 'force' | 'grid';

export interface DisplayNode {
	id: string;
	label: string;
	kind: string;
	status?: string;
	iteration?: number;
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
 * dispatches on this instead of branching on raw kind strings, so new
 * node families (notes, triggers, agent cards) extend one mapping.
 */
export type NodeRenderKind =
	'terminal' | 'decision' | 'tool' | 'trigger' | 'note' | 'agent' | 'step';

const TRIGGER_KINDS = new Set([
	'trigger',
	'TRIGGER',
	'webhook',
	'WEBHOOK',
	'schedule',
	'SCHEDULE',
	'cron',
	'CRON',
]);

const NOTE_KINDS = new Set([
	'note',
	'NOTE',
	'comment',
	'COMMENT',
	'annotation',
]);

const AGENT_KINDS = new Set(['agent', 'AGENT', 'subagent', 'SUBAGENT']);

/** Canonical render role for a backend node kind within a preset. */
export function renderKind(kind: string, preset: GraphPreset): NodeRenderKind {
	const normalized = (kind ?? '').trim().toLowerCase();
	if (TERMINAL_KINDS.has(kind) || TERMINAL_KINDS.has(normalized)) {
		return 'terminal';
	}
	if (
		normalized === 'decision' ||
		normalized === 'branch' ||
		(preset === 'decision' && (ERROR_KINDS.has(kind) || normalized === 'error'))
	) {
		return 'decision';
	}
	if (TOOL_KINDS.has(kind) || TOOL_KINDS.has(normalized)) return 'tool';
	if (TRIGGER_KINDS.has(kind) || TRIGGER_KINDS.has(normalized))
		return 'trigger';
	if (NOTE_KINDS.has(kind) || NOTE_KINDS.has(normalized)) return 'note';
	if (AGENT_KINDS.has(kind) || AGENT_KINDS.has(normalized)) return 'agent';
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

const TERMINAL_KINDS = new Set([
	'start',
	'end',
	'START',
	'END',
	'start_node',
	'end_node',
]);

const ERROR_KINDS = new Set(['error', 'ERROR', 'failed', 'FAILED']);

const TOOL_KINDS = new Set([
	'tool',
	'tool_call',
	'TOOL',
	'TOOL_CALL',
	'llm',
	'LLM',
]);

/** Cytoscape shape name for a node kind within a preset. */
export function nodeShape(kind: string, preset: GraphPreset): string {
	switch (renderKind(kind, preset)) {
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
	if (normalized === 'cached' || normalized === 'info')
		return TONE_HEX.info;
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
 * Edges survive only when both endpoints survive.
 */
export function capGraph(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	cap: number = GRAPH_NODE_CAP,
): CappedGraph {
	if (nodes.length <= cap) {
		return { nodes, edges, truncated: false, total: nodes.length };
	}
	const buckets = new Map<string, DisplayNode[]>();
	for (const node of nodes) {
		const kind = node.kind || 'unknown';
		const bucket = buckets.get(kind) ?? [];
		bucket.push(node);
		buckets.set(kind, bucket);
	}
	const kept: DisplayNode[] = [];
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

const COLUMN_GAP = 200;
const ROW_GAP = 72;

/**
 * Preset positions for decision graphs: one column per iteration, rows in
 * arrival order. Computed here so the renderer only applies them.
 */
export function columnPositions(
	nodes: DisplayNode[],
): Map<string, { x: number; y: number }> {
	const rows = new Map<number, number>();
	const positions = new Map<string, { x: number; y: number }>();
	for (const node of nodes) {
		const iteration = node.iteration ?? 0;
		const row = rows.get(iteration) ?? 0;
		rows.set(iteration, row + 1);
		positions.set(node.id, {
			x: 40 + iteration * COLUMN_GAP,
			y: 40 + row * ROW_GAP,
		});
	}
	return positions;
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

const LAYER_X_GAP = 220;
const LAYER_Y_GAP = 72;

/**
 * Layered DAG positions: depth from entry nodes via longest-path ranks, rows
 * in stable id order, snapped to a grid so edges stay axis-aligned.
 * Cyclic leftovers fall one layer past the deepest rank instead of failing.
 * Pure function; the renderer only applies the result.
 */
export function layeredPositions(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
): Map<string, { x: number; y: number }> {
	const ids = nodes.map((node) => node.id);
	const incoming = new Map<string, number>();
	const outgoing = new Map<string, string[]>();
	for (const id of ids) {
		incoming.set(id, 0);
		outgoing.set(id, []);
	}
	for (const edge of edges) {
		if (!incoming.has(edge.source) || !incoming.has(edge.target)) continue;
		if (edge.source === edge.target) continue;
		incoming.set(edge.target, (incoming.get(edge.target) ?? 0) + 1);
		outgoing.get(edge.source)?.push(edge.target);
	}
	const depth = new Map<string, number>();
	const queue: string[] = [];
	for (const id of ids) {
		if ((incoming.get(id) ?? 0) === 0) {
			depth.set(id, 0);
			queue.push(id);
		}
	}
	const remaining = new Map(incoming);
	while (queue.length > 0) {
		const current = queue.shift() as string;
		const currentDepth = depth.get(current) ?? 0;
		for (const next of outgoing.get(current) ?? []) {
			if (currentDepth + 1 > (depth.get(next) ?? -1)) {
				depth.set(next, currentDepth + 1);
			}
			remaining.set(next, (remaining.get(next) ?? 1) - 1);
			if ((remaining.get(next) ?? 0) <= 0) queue.push(next);
		}
	}
	let maxDepth = 0;
	for (const value of depth.values()) maxDepth = Math.max(maxDepth, value);
	for (const id of ids) {
		if (!depth.has(id)) {
			maxDepth += 1;
			depth.set(id, maxDepth);
		}
	}
	const layers = new Map<number, string[]>();
	for (const id of [...ids].sort()) {
		const rank = depth.get(id) ?? 0;
		const layer = layers.get(rank) ?? [];
		layer.push(id);
		layers.set(rank, layer);
	}
	const positions = new Map<string, { x: number; y: number }>();
	for (const [rank, members] of layers) {
		members.forEach((id, index) => {
			positions.set(id, {
				x: 40 + rank * LAYER_X_GAP,
				y: 40 + index * LAYER_Y_GAP,
			});
		});
	}
	return positions;
}
