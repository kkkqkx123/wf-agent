import dagre from 'dagre';
import {
	renderKind,
	type DisplayEdge,
	type DisplayNode,
} from './display-model';

const NODE_WIDTH = 120;
const TITLE_WIDTH = 160;
const NODE_HEIGHT = 40;
/** Horizontal rank separation between node bounding boxes. */
const RANK_SEP = 100;
/** Vertical separation between node bounding boxes in the same rank. */
const NODE_SEP = 40;
const COMPONENT_GAP = 120;
const NOTE_GAP = 80;
const ORIGIN = 40;
export const GRID_SIZE = 20;

/** Snap a coordinate to the grid so edges stay axis-aligned. */
export function snapToGrid(value: number, size: number = GRID_SIZE): number {
	return Math.round(value / size) * size;
}

interface FlowEdge {
	source: string;
	target: string;
}

/** Directed edges between known nodes; self loops never affect layout. */
function validEdges(
	nodes: DisplayNode[],
	edges: Array<{ source: string; target: string }>,
): FlowEdge[] {
	const known = new Set(nodes.map((node) => node.id));
	return edges
		.filter(
			(edge) =>
				edge.source !== edge.target &&
				known.has(edge.source) &&
				known.has(edge.target),
		)
		.map((edge) => ({ source: edge.source, target: edge.target }))
		.sort((a, b) =>
			a.source < b.source
				? -1
				: a.source > b.source
					? 1
					: a.target < b.target
						? -1
						: a.target > b.target
							? 1
							: 0,
		);
}

/**
 * Undirected connected components, each sorted by id and ordered by
 * smallest member. Isolated nodes form their own component.
 */
export function connectedComponents(
	nodes: DisplayNode[],
	edges: Array<{ source: string; target: string }>,
): string[][] {
	const graph = new dagre.graphlib.Graph({ directed: true });
	for (const node of [...nodes].sort((a, b) => (a.id < b.id ? -1 : 1))) {
		graph.setNode(node.id, {});
	}
	for (const edge of validEdges(nodes, edges)) {
		graph.setEdge(edge.source, edge.target);
	}
	return dagre.graphlib.alg
		.components(graph)
		.map((members) => [...members].sort())
		.sort((a, b) => (a[0]! < b[0]! ? -1 : 1));
}

function nodeWidth(node: DisplayNode): number {
	return node.kind === 'group' ? TITLE_WIDTH : NODE_WIDTH;
}

/**
 * Layered DAG positions from dagre: left-to-right ranks, deterministic
 * insertion order, components laid side by side, notes docked below the
 * component of their host node. Pure function; the renderer applies it.
 */
export function layeredPositions(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
): Map<string, { x: number; y: number }> {
	const positions = new Map<string, { x: number; y: number }>();
	if (nodes.length === 0) return positions;
	const kindOf = new Map(
		nodes.map((node) => [node.id, renderKind(node.kind, 'workflow')] as const),
	);
	const flowNodes = nodes.filter((node) => kindOf.get(node.id) !== 'note');
	const noteNodes = nodes
		.filter((node) => kindOf.get(node.id) === 'note')
		.sort((a, b) => (a.id < b.id ? -1 : 1));
	const flowById = new Map(flowNodes.map((node) => [node.id, node]));
	const flowEdges = validEdges(nodes, edges).filter(
		(edge) => flowById.has(edge.source) && flowById.has(edge.target),
	);
	const components = connectedComponents(flowNodes, flowEdges);
	const componentOf = new Map<string, number>();
	components.forEach((members, index) => {
		for (const id of members) componentOf.set(id, index);
	});
	let cursorX = ORIGIN;
	const bottoms: number[] = [];
	for (const members of components) {
		const graph = new dagre.graphlib.Graph({ directed: true });
		graph.setGraph({
			rankdir: 'LR',
			align: 'UL',
			ranker: 'longest-path',
			acyclicer: 'greedy',
			nodesep: NODE_SEP,
			ranksep: RANK_SEP,
			marginx: 0,
			marginy: 0,
		});
		graph.setDefaultEdgeLabel(() => ({}));
		for (const id of members) {
			const node = flowById.get(id);
			if (!node) continue;
			graph.setNode(id, { width: nodeWidth(node), height: NODE_HEIGHT });
		}
		for (const edge of flowEdges) {
			if (componentOf.get(edge.source) === componentOf.get(edge.target)) {
				graph.setEdge(edge.source, edge.target);
			}
		}
		dagre.layout(graph);
		let minX = Number.POSITIVE_INFINITY;
		let minY = Number.POSITIVE_INFINITY;
		let maxX = Number.NEGATIVE_INFINITY;
		let maxY = Number.NEGATIVE_INFINITY;
		for (const id of members) {
			const placed = graph.node(id);
			minX = Math.min(minX, placed.x - placed.width / 2);
			minY = Math.min(minY, placed.y - placed.height / 2);
			maxX = Math.max(maxX, placed.x + placed.width / 2);
			maxY = Math.max(maxY, placed.y + placed.height / 2);
		}
		const shiftX = cursorX - minX;
		const shiftY = ORIGIN - minY;
		for (const id of members) {
			const placed = graph.node(id);
			positions.set(id, {
				x: snapToGrid(placed.x + shiftX),
				y: snapToGrid(placed.y + shiftY),
			});
		}
		bottoms.push(snapToGrid(maxY + shiftY));
		cursorX = snapToGrid(maxX + shiftX) + COMPONENT_GAP;
	}
	const notesPerComponent = new Map<number, number>();
	const loneNotes: DisplayNode[] = [];
	for (const note of noteNodes) {
		const hosts = validEdges(nodes, edges)
			.flatMap((edge) =>
				edge.source === note.id
					? [edge.target]
					: edge.target === note.id
						? [edge.source]
						: [],
			)
			.filter((id) => flowById.has(id))
			.sort();
		const host = hosts[0];
		const component = host ? componentOf.get(host) : undefined;
		if (!host || component === undefined) {
			loneNotes.push(note);
			continue;
		}
		const count = notesPerComponent.get(component) ?? 0;
		notesPerComponent.set(component, count + 1);
		const hostPosition = positions.get(host);
		if (!hostPosition) {
			loneNotes.push(note);
			continue;
		}
		positions.set(note.id, {
			x: snapToGrid(hostPosition.x),
			y: snapToGrid((bottoms[component] ?? ORIGIN) + NOTE_GAP * (count + 1)),
		});
	}
	loneNotes.forEach((note, index) => {
		positions.set(note.id, {
			x: snapToGrid(cursorX + index * (NODE_WIDTH + NOTE_GAP)),
			y: snapToGrid(ORIGIN),
		});
	});
	return positions;
}

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
			x: snapToGrid(40 + iteration * 200),
			y: snapToGrid(40 + row * 80),
		});
	}
	return positions;
}
