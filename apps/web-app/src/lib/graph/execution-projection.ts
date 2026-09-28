import {
	rankTone,
	statusForTone,
	type DisplayEdge,
	type DisplayNode,
	type ExecutionTone,
} from './display-model';

export interface SlowNodeRef {
	node: string;
	durationMs: number;
}

export interface ExecutionOverlayInput {
	currentNode?: string | null;
	failedNodes?: string[];
	criticalPath?: string[];
	slowNodes?: Array<string | SlowNodeRef>;
	decisionPoints?: string[];
	executedNodes?: string[];
	liveStatuses?: Record<string, string>;
}

export interface ProjectedNodeMark {
	id: string;
	tone: ExecutionTone;
	pulse: boolean;
	critical: boolean;
	slow: boolean;
	heatTier: 0 | 1 | 2 | 3;
	decision: boolean;
}

export interface ExecutionOverlay {
	marks: Map<string, ProjectedNodeMark>;
}

/**
 * Heat tier for a slow node duration relative to the slowest node.
 * Tertiles of the max keep small graphs stable; tiny samples collapse to
 * a single tier. Pure function for unit tests.
 */
export function slowHeatTier(
	durationMs: number,
	maxDurationMs: number,
): 0 | 1 | 2 | 3 {
	if (!Number.isFinite(durationMs) || durationMs <= 0) return 0;
	if (!Number.isFinite(maxDurationMs) || maxDurationMs <= 0) return 1;
	const ratio = durationMs / maxDurationMs;
	if (ratio > 2 / 3) return 3;
	if (ratio > 1 / 3) return 2;
	return 1;
}

/**
 * Merge execution, analysis and live signals into one mark per node.
 * Priority is running above error above warning above success; the canvas
 * only renders marks and never reads raw execution payloads.
 */
export function projectExecutionOverlay(
	nodes: DisplayNode[],
	input: ExecutionOverlayInput,
): ExecutionOverlay {
	const marks = new Map<string, ProjectedNodeMark>();
	const failed = new Set(input.failedNodes ?? []);
	const critical = new Set(input.criticalPath ?? []);
	const decisions = new Set(input.decisionPoints ?? []);
	const executed = new Set(input.executedNodes ?? []);
	const slowDurations = new Map<string, number>();
	for (const entry of input.slowNodes ?? []) {
		if (typeof entry === 'string') {
			if (!slowDurations.has(entry)) slowDurations.set(entry, 0);
		} else if (!slowDurations.has(entry.node)) {
			slowDurations.set(entry.node, entry.durationMs);
		}
	}
	const maxSlow = Math.max(0, ...slowDurations.values());
	const slow = new Set(slowDurations.keys());
	for (const node of nodes) {
		let tone: ExecutionTone = 'neutral';
		const live = input.liveStatuses?.[node.id];
		if (live !== undefined) {
			tone = rankTone(tone, toneForLive(live));
		}
		if (executed.has(node.id)) tone = rankTone(tone, 'success');
		if (slow.has(node.id)) tone = rankTone(tone, 'warning');
		if (failed.has(node.id)) tone = rankTone(tone, 'error');
		if (input.currentNode === node.id) tone = rankTone(tone, 'running');
		const duration = slowDurations.get(node.id);
		marks.set(node.id, {
			id: node.id,
			tone,
			pulse: input.currentNode === node.id,
			critical: critical.has(node.id),
			slow: slow.has(node.id),
			heatTier:
				duration === undefined
					? 0
					: duration <= 0
						? 1
						: slowHeatTier(duration, maxSlow),
			decision: decisions.has(node.id),
		});
	}
	return { marks };
}

function toneForLive(status: string): ExecutionTone {
	const normalized = status.trim().toLowerCase();
	if (['running', 'pending'].includes(normalized))
		return normalized as ExecutionTone;
	if (normalized === 'failed') return 'error';
	if (normalized === 'completed') return 'success';
	return 'neutral';
}

/**
 * Apply overlay marks to display nodes. Base statuses survive underneath;
 * the overlay only upgrades tones, never downgrades a node to neutral.
 */
export function applyExecutionOverlay(
	nodes: DisplayNode[],
	overlay: ExecutionOverlay,
): DisplayNode[] {
	return nodes.map((node) => {
		const mark = overlay.marks.get(node.id);
		if (!mark || mark.tone === 'neutral') return node;
		return { ...node, status: statusForTone(mark.tone) ?? node.status };
	});
}

/**
 * Edge tone derives from its source node: failed sources paint the edge
 * failed, completed sources with an executed target paint it completed.
 */
export function projectEdgeTone(
	sourceTone: ExecutionTone,
	targetExecuted: boolean,
): ExecutionTone {
	if (sourceTone === 'error') return 'error';
	if (sourceTone === 'running') return 'running';
	if (sourceTone === 'success' && targetExecuted) return 'success';
	return 'neutral';
}

/**
 * Derive one tone per edge from node marks. A target counts as executed
 * when its mark left neutral, so completed chains paint through while
 * dangling success never leaks onto unvisited edges.
 */
export function projectEdgeOverlay(
	edges: DisplayEdge[],
	overlay: ExecutionOverlay,
): Map<string, ExecutionTone> {
	const tones = new Map<string, ExecutionTone>();
	for (const edge of edges) {
		const sourceTone = overlay.marks.get(edge.source)?.tone ?? 'neutral';
		const targetTone = overlay.marks.get(edge.target)?.tone ?? 'neutral';
		tones.set(edge.id, projectEdgeTone(sourceTone, targetTone !== 'neutral'));
	}
	return tones;
}

/**
 * Apply edge tones to display edges. Neutral tones leave the edge
 * untouched so the canvas keeps its default styling.
 */
export function applyEdgeOverlay(
	edges: DisplayEdge[],
	tones: Map<string, ExecutionTone>,
): DisplayEdge[] {
	return edges.map((edge) => {
		const tone = tones.get(edge.id);
		if (!tone || tone === 'neutral') return edge;
		return { ...edge, status: statusForTone(tone) ?? edge.status };
	});
}

export interface DiffEdgeRef {
	source: string;
	target: string;
}

export interface TopologyDiff {
	addedNodes: string[];
	removedNodes: string[];
	addedEdges: DiffEdgeRef[];
	removedEdges: DiffEdgeRef[];
}

/**
 * Structural diff between two topologies, computed client-side. The result
 * feeds the projection layer as an overlay; neither input is mutated.
 * Edges compare by source-target pairs and carry their endpoints so the
 * version graph can render added and removed edges, not just list them.
 */
export function diffTopology(
	beforeNodes: DisplayNode[],
	beforeEdges: DisplayEdge[],
	afterNodes: DisplayNode[],
	afterEdges: DisplayEdge[],
): TopologyDiff {
	const beforeNodeIds = new Set(beforeNodes.map((node) => node.id));
	const afterNodeIds = new Set(afterNodes.map((node) => node.id));
	const key = (edge: DisplayEdge): string => `${edge.source}->${edge.target}`;
	const ref = (edge: DisplayEdge): DiffEdgeRef => ({
		source: edge.source,
		target: edge.target,
	});
	const beforeEdgeKeys = new Set(beforeEdges.map(key));
	const afterEdgeKeys = new Set(afterEdges.map(key));
	return {
		addedNodes: [...afterNodeIds].filter((id) => !beforeNodeIds.has(id)),
		removedNodes: [...beforeNodeIds].filter((id) => !afterNodeIds.has(id)),
		addedEdges: afterEdges
			.filter((edge) => !beforeEdgeKeys.has(key(edge)))
			.map(ref),
		removedEdges: beforeEdges
			.filter((edge) => !afterEdgeKeys.has(key(edge)))
			.map(ref),
	};
}

/**
 * Whether a dotted issue field path targets a node id (segment match, so
 * `nodes.b.name` hits `b` but `nodesbly.name` does not).
 */
export function issueTargetsNode(field: string, nodeId: string): boolean {
	return field.split(/[^A-Za-z0-9_-]+/).includes(nodeId);
}

/**
 * Map server validation issues to node ids by matching dotted field-path
 * segments against known node ids. Unmatched issues stay global.
 */
export function issueNodeIds(
	issues: Array<{ field: string; message: string }>,
	nodes: DisplayNode[],
): Map<string, string[]> {
	const ids = new Set(nodes.map((node) => node.id));
	const matched = new Map<string, string[]>();
	for (const issue of issues) {
		for (const id of ids) {
			if (!issueTargetsNode(issue.field, id)) continue;
			const list = matched.get(id) ?? [];
			list.push(issue.message);
			matched.set(id, list);
		}
	}
	return matched;
}
