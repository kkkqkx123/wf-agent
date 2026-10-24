import type { ElementDefinition } from 'cytoscape';
import {
	edgeGradient,
	isDashedEdge,
	isFailureBranch,
	isWaitingStatus,
	nodeShape,
	resolveStatusHex,
	scoreEdgeLabel,
	shortLabel,
	type DisplayEdge,
	type DisplayNode,
	type GraphPreset,
} from './display-model';
import { isGroupTitleId, type GroupTitle } from './group-view';
import type { CanvasPosition } from './canvas-model';

export interface ElementBuildInput {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
	preset: GraphPreset;
	positions: Record<string, CanvasPosition> | undefined;
	computed: Record<string, CanvasPosition>;
	collapsed: Set<string>;
	highlight: Set<string>;
	problems: Set<string>;
	pulses: Set<string>;
	failed: Set<string>;
	criticals: Set<string>;
	decisions: Set<string>;
	heatTierById: Record<string, number>;
	selectedId: string | null;
	groupTitles: Record<string, GroupTitle>;
	edgeLabelLimit: number;
	zoomedOut: boolean;
	rankedEdgeIds: Set<string>;
}

/**
 * Ranked ids of the edge labels worth keeping on over-budget graphs.
 * Pure scoring; callers cache the result per graph revision.
 */
export function rankEdgeIds(
	edges: DisplayEdge[],
	edgeLabelLimit: number,
	selectedId: string | null,
	highlight: Set<string>,
): Set<string> {
	const scored = edges
		.filter(
			(edge) =>
				(edge.label ?? '').trim() &&
				!(
					selectedId !== null &&
					(edge.source === selectedId || edge.target === selectedId)
				),
		)
		.map((edge) => ({
			id: edge.id,
			score:
				scoreEdgeLabel(edge) +
				(highlight.has(edge.source) || highlight.has(edge.target) ? 50 : 0),
		}))
		.sort((a, b) => b.score - a.score)
		.slice(0, edgeLabelLimit)
		.map((entry) => entry.id);
	return new Set(scored);
}

/**
 * Label visibility for one edge. Selection-adjacent labels always show.
 * Zoomed-out canvases hide the rest. Over-budget graphs rank the rest
 * by importance and keep only the top slice.
 */
export function edgeLabelFor(
	edge: DisplayEdge,
	touchesSelection: boolean,
	zoomedOut: boolean,
	edgeCount: number,
	edgeLabelLimit: number,
	rankedEdgeIds: Set<string>,
): string {
	const label = edge.label ?? '';
	if (!label) return '';
	if (touchesSelection) return label;
	if (zoomedOut) return '';
	if (edgeCount <= edgeLabelLimit) return label;
	return rankedEdgeIds.has(edge.id) ? label : '';
}

/**
 * Comma-form color for gradient stops: space-separated `hsl()` channels
 * would split into extra stops, so channels join with commas instead.
 */
function gradientStop(color: string): string {
	const match = /^hsl\(([^)]+)\)$/.exec(color.trim());
	if (!match) return color;
	const channels = match[1].split(/[\s,]+/).filter(Boolean);
	if (channels.length < 3) return color;
	const alpha = channels.slice(3).join(' ');
	return alpha
		? `hsl(${channels[0]},${channels[1]},${channels[2]} / ${alpha})`
		: `hsl(${channels[0]},${channels[1]},${channels[2]})`;
}

function titleEmphasis(
	id: string,
	groupTitles: Record<string, GroupTitle>,
	pulses: Set<string>,
	failed: Set<string>,
	problems: Set<string>,
	criticals: Set<string>,
): string[] {
	if (!isGroupTitleId(id)) return [];
	const title = groupTitles[id];
	const members = title?.memberIds ?? [];
	if (members.length === 0) return [];
	if (members.some((member) => pulses.has(member))) return ['running'];
	if (members.some((member) => failed.has(member))) return ['failed'];
	if (members.some((member) => problems.has(member))) return ['problem'];
	if (members.some((member) => criticals.has(member))) return ['critical'];
	return [];
}

/** Cytoscape element definitions for the current graph revision. */
export function buildElementDefs(
	input: ElementBuildInput,
): ElementDefinition[] {
	const {
		nodes,
		edges,
		preset,
		positions,
		computed,
		collapsed,
		highlight,
		problems,
		pulses,
		failed,
		criticals,
		decisions,
		heatTierById,
		selectedId,
		groupTitles,
		edgeLabelLimit,
		zoomedOut,
		rankedEdgeIds,
	} = input;
	// Expanded groups render as compound parent boxes; collapsed groups
	// arrive pre-folded as title nodes with their members excluded.
	const boxIds = [
		...new Set(
			nodes.flatMap((node) =>
				node.groupId && !collapsed.has(node.groupId) && !isGroupTitleId(node.id)
					? [node.groupId]
					: [],
			),
		),
	];

	const defs: ElementDefinition[] = nodes.map((node) => ({
		group: 'nodes' as const,
		data: {
			id: node.id,
			label:
				isGroupTitleId(node.id) && groupTitles[node.id]
					? `${groupTitles[node.id].label} (${groupTitles[node.id].memberIds.length})`
					: shortLabel(node.label),
			fullLabel: node.label,
			kind: node.kind,
			shape: nodeShape(node.kind, preset),
			color: resolveStatusHex(node.status),
			...(node.groupId && boxIds.includes(node.groupId)
				? { parent: `groupbox:${node.groupId}` }
				: {}),
		},
		position: positions?.[node.id] ?? computed[node.id],
		classes: [
			selectedId === node.id ? 'selected' : '',
			isGroupTitleId(node.id) ? 'group-title' : '',
			...titleEmphasis(
				node.id,
				groupTitles,
				pulses,
				failed,
				problems,
				criticals,
			),
			problems.has(node.id) ? 'problem' : '',
			pulses.has(node.id) ? 'running' : '',
			failed.has(node.id) ? 'failed' : '',
			criticals.has(node.id) ? 'critical' : '',
			decisions.has(node.id) ? 'decision' : '',
			heatTierById[node.id] === 3
				? 'heat-3'
				: heatTierById[node.id] === 2
					? 'heat-2'
					: heatTierById[node.id] === 1
						? 'heat-1'
						: '',
			highlight.size > 0
				? highlight.has(node.id)
					? 'highlighted'
					: 'dimmed'
				: '',
		]
			.filter(Boolean)
			.join(' '),
	}));
	for (const groupId of boxIds) {
		defs.push({
			group: 'nodes' as const,
			data: { id: `groupbox:${groupId}`, label: groupId },
			classes: 'group-box',
		});
	}
	const statusById = new Map(
		nodes.map((node) => [node.id, node.status]),
	);
	for (const edge of edges) {
		const touchesSelection =
			selectedId !== null &&
			(edge.source === selectedId || edge.target === selectedId);
		const failBranch = isFailureBranch(edge);
		const waiting = isWaitingStatus(edge.status);
		const gradient = edgeGradient(edge, statusById);
		defs.push({
			group: 'edges' as const,
			data: {
				id: edge.id,
				source: edge.source,
				target: edge.target,
				label: edgeLabelFor(
					edge,
					touchesSelection,
					zoomedOut,
					edges.length,
					edgeLabelLimit,
					rankedEdgeIds,
				),
				lineStyle: isDashedEdge(edge.kind, edge.taken ?? true)
					? 'dashed'
					: 'solid',
				color: resolveStatusHex(edge.status),
				...(gradient
					? {
							gradient: `${gradientStop(gradient.from)} ${gradientStop(gradient.to)}`,
							targetColor: gradientStop(gradient.to),
						}
					: {}),
			},
			classes: [
				failBranch ? 'fail-branch' : '',
				gradient ? 'gradient' : '',
				waiting || gradient?.waiting ? 'waiting' : '',
			]
				.filter(Boolean)
				.join(' '),
		});
	}
	return defs;
}
