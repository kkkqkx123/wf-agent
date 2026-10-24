import type { DisplayEdge, DisplayNode } from './display-model';

const NODE_WIDTH = 120;
const TITLE_WIDTH = 160;
const NODE_HEIGHT = 40;
/** Horizontal rank separation between node bounding boxes. */
const RANK_SEP = 100;
/** Vertical separation between node bounding boxes in the same rank. */
const NODE_SEP = 40;
const COMPONENT_GAP = 120;
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
	const adjacency = new Map<string, string[]>();
	for (const node of nodes) adjacency.set(node.id, []);
	for (const edge of validEdges(nodes, edges)) {
		adjacency.get(edge.source)?.push(edge.target);
		adjacency.get(edge.target)?.push(edge.source);
	}
	const visited = new Set<string>();
	const components: string[][] = [];
	const sortedNodes = [...nodes].sort((a, b) =>
		a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
	);
	for (const node of sortedNodes) {
		if (visited.has(node.id)) continue;
		const stack = [node.id];
		const component: string[] = [];
		while (stack.length > 0) {
			const current = stack.pop()!;
			if (visited.has(current)) continue;
			visited.add(current);
			component.push(current);
			for (const next of adjacency.get(current) ?? []) {
				if (!visited.has(next)) stack.push(next);
			}
		}
		component.sort();
		components.push(component);
	}
	components.sort((a, b) => (a[0]! < b[0]! ? -1 : a[0]! > b[0]! ? 1 : 0));
	return components;
}

function nodeWidth(node: DisplayNode): number {
	return node.kind === 'group' ? TITLE_WIDTH : NODE_WIDTH;
}

/**
 * Layered DAG positions from dagre: left-to-right ranks, deterministic
 * insertion order, disconnected components laid side by side. Pure function;
 * the renderer applies it.
 */
export function layeredPositions(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
): Map<string, { x: number; y: number }> {
	const positions = new Map<string, { x: number; y: number }>();
	if (nodes.length === 0) return positions;
	const flowNodes = nodes;
	const flowById = new Map(flowNodes.map((node) => [node.id, node]));
	const flowEdges = validEdges(nodes, edges).filter(
		(edge) => flowById.has(edge.source) && flowById.has(edge.target),
	);
	const components = connectedComponents(flowNodes, flowEdges);
	const componentOf = new Map<string, number>();
	components.forEach((members, index) => {
		for (const id of members) componentOf.set(id, index);
	});
	const ranks = nodeRanks(flowNodes, flowEdges);
	let cursorX = ORIGIN;
	for (const members of components) {
		const byRank = new Map<number, string[]>();
		for (const id of members) {
			const rank = ranks.get(id) ?? 0;
			const list = byRank.get(rank) ?? [];
			list.push(id);
			byRank.set(rank, list);
		}
		const sortedRanks = [...byRank.keys()].sort((a, b) => a - b);
		let maxMembers = 1;
		for (const list of byRank.values()) {
			maxMembers = Math.max(maxMembers, list.length);
		}
		const rankSpan = maxMembers * NODE_HEIGHT + (maxMembers - 1) * NODE_SEP;
		let minX = Number.POSITIVE_INFINITY;
		let maxX = Number.NEGATIVE_INFINITY;
		let minY = Number.POSITIVE_INFINITY;
		let maxY = Number.NEGATIVE_INFINITY;
		for (const rank of sortedRanks) {
			const membersInRank = [...(byRank.get(rank) ?? [])].sort();
			const rankHeight =
				membersInRank.length * NODE_HEIGHT +
				(membersInRank.length - 1) * NODE_SEP;
			const startY = ORIGIN + (rankSpan - rankHeight) / 2;
			membersInRank.forEach((id, index) => {
				const x = cursorX + rank * (NODE_WIDTH + RANK_SEP);
				const y = startY + index * (NODE_HEIGHT + NODE_SEP);
				positions.set(id, {
					x: snapToGrid(x),
					y: snapToGrid(y),
				});
			});
		}
		for (const id of members) {
			const placed = positions.get(id);
			if (!placed) continue;
			const node = flowById.get(id);
			const width = node ? nodeWidth(node) : NODE_WIDTH;
			minX = Math.min(minX, placed.x - width / 2);
			minY = Math.min(minY, placed.y - NODE_HEIGHT / 2);
			maxX = Math.max(maxX, placed.x + width / 2);
			maxY = Math.max(maxY, placed.y + NODE_HEIGHT / 2);
		}
		cursorX = snapToGrid(maxX) + COMPONENT_GAP;
		void minY;
		void maxY;
		void minX;
	}
	return positions;
}

/**
 * Preset positions for decision graphs: one column per iteration, rows in
 * arrival order. Computed here so the renderer only applies them.
 */
export function columnPositions(
	nodes: DisplayNode[],
	edges: Array<{ source: string; target: string }> = [],
): Map<string, { x: number; y: number }> {
	const positions = new Map<string, { x: number; y: number }>();
	if (nodes.length === 0) return positions;
	const ranks = nodeRanks(nodes, edges);
	const byRank = new Map<number, DisplayNode[]>();
	for (const node of nodes) {
		const rank = ranks.get(node.id) ?? 0;
		const list = byRank.get(rank) ?? [];
		list.push(node);
		byRank.set(rank, list);
	}
	const sortedRanks = [...byRank.keys()].sort((a, b) => a - b);
	let rankIndex = 0;
	for (const rank of sortedRanks) {
		const members = [...(byRank.get(rank) ?? [])].sort((a, b) =>
			a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
		);
		members.forEach((node, index) => {
			positions.set(node.id, {
				x: snapToGrid(40 + rankIndex * 200),
				y: snapToGrid(40 + index * 80),
			});
		});
		rankIndex += 1;
	}
	return positions;
}

export function rowPositions(
	nodes: DisplayNode[],
	edges: Array<{ source: string; target: string }> = [],
): Map<string, { x: number; y: number }> {
	const positions = new Map<string, { x: number; y: number }>();
	if (nodes.length === 0) return positions;
	const ranks = nodeRanks(nodes, edges);
	const byRank = new Map<number, DisplayNode[]>();
	for (const node of nodes) {
		const rank = ranks.get(node.id) ?? 0;
		const list = byRank.get(rank) ?? [];
		list.push(node);
		byRank.set(rank, list);
	}
	const sortedRanks = [...byRank.keys()].sort((a, b) => a - b);
	let rankIndex = 0;
	for (const rank of sortedRanks) {
		const members = [...(byRank.get(rank) ?? [])].sort((a, b) =>
			a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
		);
		members.forEach((node, index) => {
			positions.set(node.id, {
				x: snapToGrid(40 + index * 160),
				y: snapToGrid(40 + rankIndex * 100),
			});
		});
		rankIndex += 1;
	}
	return positions;
}

export function compactPositions(
	nodes: DisplayNode[],
	edges: Array<{ source: string; target: string }> = [],
	columns: number = 4,
): Map<string, { x: number; y: number }> {
	const positions = new Map<string, { x: number; y: number }>();
	if (nodes.length === 0) return positions;
	const order = topologicalOrder(nodes, edges);
	const safeColumns = Math.max(1, Math.floor(columns));
	order.forEach((node, index) => {
		const column = index % safeColumns;
		const row = Math.floor(index / safeColumns);
		positions.set(node.id, {
			x: snapToGrid(40 + column * 180),
			y: snapToGrid(40 + row * 90),
		});
	});
	return positions;
}

/** Topological rank per node: longest path from any source. Falls back to
 *  iteration when present (decision graphs). */
function nodeRanks(
	nodes: DisplayNode[],
	edges: Array<{ source: string; target: string }>,
): Map<string, number> {
	const ranks = new Map<string, number>();
	if (nodes.some((node) => node.iteration !== undefined)) {
		for (const node of nodes) ranks.set(node.id, node.iteration ?? 0);
		return ranks;
	}
	for (const node of nodes) ranks.set(node.id, 0);
	const known = new Set(nodes.map((node) => node.id));
	const filtered = edges.filter(
		(edge) =>
			known.has(edge.source) &&
			known.has(edge.target) &&
			edge.source !== edge.target,
	);
	for (let i = 0; i < nodes.length; i += 1) {
		let changed = false;
		for (const edge of filtered) {
			const sourceRank = ranks.get(edge.source) ?? 0;
			const targetRank = ranks.get(edge.target) ?? 0;
			if (targetRank < sourceRank + 1) {
				ranks.set(edge.target, sourceRank + 1);
				changed = true;
			}
		}
		if (!changed) break;
	}
	return ranks;
}

function topologicalOrder(
	nodes: DisplayNode[],
	edges: Array<{ source: string; target: string }>,
): DisplayNode[] {
	const ranks = nodeRanks(nodes, edges);
	return [...nodes].sort((a, b) => {
		const rankDiff = (ranks.get(a.id) ?? 0) - (ranks.get(b.id) ?? 0);
		if (rankDiff !== 0) return rankDiff;
		return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
	});
}

export interface CanvasPoint {
	x: number;
	y: number;
}

export interface CanvasMoveInput {
	id: string;
	position: CanvasPoint;
}

interface Box {
	x1: number;
	y1: number;
	x2: number;
	y2: number;
}

function nodeBox(position: CanvasPoint, width: number, height: number): Box {
	return {
		x1: position.x - width / 2,
		y1: position.y - height / 2,
		x2: position.x + width / 2,
		y2: position.y + height / 2,
	};
}

function boxesOverlap(a: Box, b: Box): boolean {
	return a.x1 < b.x2 && a.x2 > b.x1 && a.y1 < b.y2 && a.y2 > b.y1;
}

/**
 * Push stationary nodes out of moved boxes. Returns extra moves for the
 * pushed nodes so the caller can persist everything in one commit. Pure
 * function; the canvas supplies geometry and the store persists the union.
 */
export function pushOverlapped(
	moves: CanvasMoveInput[],
	positions: Record<string, CanvasPoint>,
	options?: { width?: number; height?: number; gap?: number },
): CanvasMoveInput[] {
	const width = options?.width ?? NODE_WIDTH;
	const height = options?.height ?? NODE_HEIGHT;
	const gap = options?.gap ?? 24;
	if (moves.length === 0) return [];
	const movedIds = new Set(moves.map((move) => move.id));
	const current = new Map<string, CanvasPoint>();
	for (const [id, position] of Object.entries(positions)) {
		current.set(id, { ...position });
	}
	for (const move of moves) current.set(move.id, { ...move.position });
	const pushed: CanvasMoveInput[] = [];
	for (const move of moves) {
		const mover = nodeBox(move.position, width + gap, height + gap);
		for (const [id, position] of current) {
			if (movedIds.has(id)) continue;
			if (pushed.some((entry) => entry.id === id)) continue;
			const other = nodeBox(position, width + gap, height + gap);
			if (!boxesOverlap(mover, other)) continue;
			const shiftX = mover.x2 - other.x1 + gap;
			const shiftLeft = other.x2 - mover.x1 + gap;
			const shiftY = mover.y2 - other.y1 + gap;
			const shiftUp = other.y2 - mover.y1 + gap;
			const minX = Math.min(shiftX, shiftLeft);
			const minY = Math.min(shiftY, shiftUp);
			const next =
				minX < minY
					? {
							x: position.x + (shiftX < shiftLeft ? shiftX : -shiftLeft),
							y: position.y,
						}
					: {
							x: position.x,
							y: position.y + (shiftY < shiftUp ? shiftY : -shiftUp),
						};
			const snapped = {
				x: snapToGrid(Math.round(next.x)),
				y: snapToGrid(Math.round(next.y)),
			};
			pushed.push({ id, position: snapped });
			current.set(id, snapped);
		}
	}
	return pushed;
}

/**
 * Layered layout honoring expanded-group membership. Each multi-member
 * group lays out internally first, then participates outside as one box,
 * so large groups stay compact instead of scattering. Pure function.
 */
export function groupAwareLayeredPositions(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	parentOf: Record<string, string>,
): Map<string, { x: number; y: number }> {
	const groups = new Map<string, DisplayNode[]>();
	const ungrouped: DisplayNode[] = [];
	for (const node of nodes) {
		const parent = parentOf[node.id];
		if (parent) {
			const list = groups.get(parent) ?? [];
			list.push(node);
			groups.set(parent, list);
		} else {
			ungrouped.push(node);
		}
	}
	if (groups.size === 0) return layeredPositions(nodes, edges);
	const byId = new Map(nodes.map((node) => [node.id, node]));
	const outerNodes: DisplayNode[] = [...ungrouped];
	const outerEdges: DisplayEdge[] = [];
	const innerByGroup = new Map<string, Map<string, { x: number; y: number }>>();
	const innerSize = new Map<string, { w: number; h: number }>();
	for (const [groupId, members] of groups) {
		if (members.length < 2) {
			outerNodes.push(...members);
			continue;
		}
		void byId;
		const inner = layeredPositions(
			members,
			edges.filter(
				(edge) =>
					members.some((member) => member.id === edge.source) &&
					members.some((member) => member.id === edge.target),
			),
		);
		innerByGroup.set(groupId, inner);
		let minX = Number.POSITIVE_INFINITY;
		let minY = Number.POSITIVE_INFINITY;
		let maxX = Number.NEGATIVE_INFINITY;
		let maxY = Number.NEGATIVE_INFINITY;
		for (const member of members) {
			const position = inner.get(member.id);
			if (!position) continue;
			minX = Math.min(minX, position.x - nodeWidth(member) / 2);
			minY = Math.min(minY, position.y - NODE_HEIGHT / 2);
			maxX = Math.max(maxX, position.x + nodeWidth(member) / 2);
			maxY = Math.max(maxY, position.y + NODE_HEIGHT / 2);
		}
		if (!Number.isFinite(minX)) {
			outerNodes.push(...members);
			innerByGroup.delete(groupId);
			continue;
		}
		innerSize.set(groupId, {
			w: Math.max(NODE_WIDTH, maxX - minX + COMPONENT_GAP / 2),
			h: Math.max(NODE_HEIGHT, maxY - minY + COMPONENT_GAP / 2),
		});
		outerNodes.push({ id: `__group:${groupId}`, label: groupId, kind: 'step' });
	}
	const outerId = (id: string): string => {
		const parent = parentOf[id];
		if (parent && innerByGroup.has(parent)) return `__group:${parent}`;
		return id;
	};
	const seen = new Set<string>();
	for (const edge of edges) {
		const source = outerId(edge.source);
		const target = outerId(edge.target);
		if (source === target) continue;
		const key = `${source}->${target}:${edge.id}`;
		if (seen.has(key)) continue;
		seen.add(key);
		outerEdges.push({ id: `outer:${edge.id}`, source, target });
	}
	const outer = layeredPositions(outerNodes, outerEdges);
	const positions = new Map<string, { x: number; y: number }>();
	for (const node of ungrouped) {
		const position = outer.get(node.id);
		if (position) positions.set(node.id, position);
	}
	for (const [groupId, members] of groups) {
		const inner = innerByGroup.get(groupId);
		const center = outer.get(`__group:${groupId}`);
		if (!inner || !center) {
			for (const member of members) {
				const fallback = outer.get(member.id);
				if (fallback) positions.set(member.id, fallback);
			}
			continue;
		}
		let sumX = 0;
		let sumY = 0;
		let count = 0;
		for (const member of members) {
			const position = inner.get(member.id);
			if (!position) continue;
			sumX += position.x;
			sumY += position.y;
			count += 1;
		}
		if (count === 0) continue;
		const shiftX = center.x - sumX / count;
		const shiftY = center.y - sumY / count;
		for (const member of members) {
			const position = inner.get(member.id);
			if (!position) continue;
			positions.set(member.id, {
				x: snapToGrid(Math.round(position.x + shiftX)),
				y: snapToGrid(Math.round(position.y + shiftY)),
			});
		}
	}
	void innerSize;
	return positions;
}

/**
 * Stable match order by canvas geometry: left to right, then top to
 * bottom. Ids without a position keep their input relative order so
 * force and grid layouts never throw.
 */
export function sortIdsByCanvasPosition(
	ids: string[],
	positions: Map<string, CanvasPoint> | Record<string, CanvasPoint> | undefined,
): string[] {
	if (!positions) return [...ids];
	const lookup = (id: string): CanvasPoint | undefined =>
		positions instanceof Map ? positions.get(id) : positions[id];
	return ids
		.map((id, index) => ({ id, index, point: lookup(id) }))
		.sort((a, b) => {
			if (a.point && b.point) {
				if (a.point.x !== b.point.x) return a.point.x - b.point.x;
				if (a.point.y !== b.point.y) return a.point.y - b.point.y;
			}
			return a.index - b.index;
		})
		.map((entry) => entry.id);
}
