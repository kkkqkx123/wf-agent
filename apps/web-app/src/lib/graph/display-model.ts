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
}

export interface LegendEntry {
	label: string;
	shape: 'ellipse' | 'diamond' | 'rounded' | 'hexagon' | 'line-solid' | 'line-dashed';
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
	const normalized = (kind ?? '').trim().toLowerCase();
	if (TERMINAL_KINDS.has(kind) || TERMINAL_KINDS.has(normalized)) {
		return 'ellipse';
	}
	if (
		preset === 'decision' &&
		(ERROR_KINDS.has(kind) || ERROR_KINDS.has(normalized))
	) {
		return 'diamond';
	}
	if (TOOL_KINDS.has(kind) || TOOL_KINDS.has(normalized)) {
		return preset === 'workflow' ? 'round-rectangle' : 'hexagon';
	}
	if (normalized === 'decision' || normalized === 'branch') return 'diamond';
	return 'round-rectangle';
}

/** Whether an edge renders dashed (conditional, error-route, untaken). */
export function isDashedEdge(kind: string | undefined, taken: boolean = true): boolean {
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
	if (
		['completed', 'complete', 'success', 'succeeded', 'done', 'ok', 'active', 'enabled'].includes(
			normalized,
		)
	) {
		return TONE_HEX.success;
	}
	if (
		['failed', 'failure', 'error', 'errored', 'timeout', 'aborted'].includes(
			normalized,
		)
	) {
		return TONE_HEX.danger;
	}
	if (['running', 'in_progress', 'executing', 'streaming', 'started'].includes(normalized)) {
		return TONE_HEX.running;
	}
	if (['paused', 'pending', 'queued', 'waiting', 'retrying'].includes(normalized)) {
		return TONE_HEX.warning;
	}
	if (normalized === 'cached' || normalized === 'info') return TONE_HEX.info;
	return TONE_HEX.neutral;
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

/** Maximum nodes rendered; larger graphs show a truncation notice. */
export const GRAPH_NODE_CAP = 800;

export interface CappedGraph {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
	truncated: boolean;
	total: number;
}

/** Enforce the node cap, keeping edges whose endpoints survive. */
export function capGraph(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	cap: number = GRAPH_NODE_CAP,
): CappedGraph {
	if (nodes.length <= cap) {
		return { nodes, edges, truncated: false, total: nodes.length };
	}
	const kept = new Set(nodes.slice(0, cap).map((node) => node.id));
	return {
		nodes: nodes.slice(0, cap),
		edges: edges.filter((edge) => kept.has(edge.source) && kept.has(edge.target)),
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
