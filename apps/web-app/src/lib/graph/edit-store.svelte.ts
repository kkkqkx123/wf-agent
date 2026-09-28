import type { DisplayEdge, DisplayNode } from './display-model';
import { groupTitleId } from './group-view';
import { SvelteSet } from 'svelte/reactivity';

export interface GraphPosition {
	x: number;
	y: number;
}

export interface GraphMove {
	id: string;
	position: GraphPosition;
}

interface HistoryEntry {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
}

function snapshot(nodes: DisplayNode[], edges: DisplayEdge[]): HistoryEntry {
	return {
		nodes: nodes.map((node) => ({ ...node })),
		edges: edges.map((edge) => ({ ...edge })),
	};
}

/** Backend edge type for a display edge kind; conditional and error routes survive round-trips. */
function draftEdgeType(kind: string | undefined): string {
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

/**
 * Controlled edit state for the graph canvas. The canvas only emits intents
 * (move, add, delete); every mutation lands here, and the canvas re-renders
 * from this single source. Undo and redo replay whole snapshots so grouped
 * drag gestures collapse to one history entry.
 */
export class GraphEditStore {
	nodes = $state<DisplayNode[]>([]);
	edges = $state<DisplayEdge[]>([]);
	positions = $state<Record<string, GraphPosition>>({});
	selectedId = $state<string | null>(null);

	private baseline = $state('');
	private past = $state<HistoryEntry[]>([]);
	private future = $state<HistoryEntry[]>([]);

	get dirty(): boolean {
		return this.describe() !== this.baseline;
	}

	get canUndo(): boolean {
		return this.past.length > 0;
	}

	get canRedo(): boolean {
		return this.future.length > 0;
	}

	load(nodes: DisplayNode[], edges: DisplayEdge[]): void {
		this.nodes = nodes.map((node) => ({ ...node }));
		this.edges = edges.map((edge) => ({ ...edge }));
		this.prunePositions();
		this.past = [];
		this.future = [];
		this.baseline = this.describe();
		this.selectedId = null;
	}

	markClean(): void {
		this.baseline = this.describe();
	}

	applyMove(
		id: string,
		position: GraphPosition,
		options?: { hiddenIds?: Iterable<string> },
	): string | null {
		if (!this.nodes.some((node) => node.id === id)) return 'Unknown node.';
		if (options?.hiddenIds && new Set(options.hiddenIds).has(id)) {
			return 'Hidden group members cannot move.';
		}
		this.commit();
		this.positions = { ...this.positions, [id]: { ...position } };
		return null;
	}

	/** Distinct group ids present in the loaded nodes, in stable order. */
	groupIds(): string[] {
		const ids = new SvelteSet<string>();
		for (const node of this.nodes) {
			if (node.groupId) ids.add(node.groupId);
		}
		return [...ids].sort();
	}

	/** Member node ids of a group, in stable order. */
	membersOf(groupId: string): string[] {
		return this.nodes
			.filter((node) => node.groupId === groupId)
			.map((node) => node.id)
			.sort();
	}

	/**
	 * Apply several position updates as one history entry, so group drags
	 * (title plus members) undo in a single step. Unknown ids are ignored.
	 * Hidden members are rejected and reported for a prompt. Pushed nodes
	 * belong in the same call so overlap resolution undoes atomically.
	 */
	applyMoves(
		moves: GraphMove[],
		options?: { hiddenIds?: Iterable<string> },
	): string[] {
		const hidden = options?.hiddenIds ? new Set(options.hiddenIds) : null;
		const rejected: string[] = [];
		const targets = moves.filter((move) => {
			if (!this.isPositionTarget(move.id)) return false;
			if (hidden?.has(move.id)) {
				rejected.push(move.id);
				return false;
			}
			return true;
		});
		if (targets.length === 0) return rejected;
		this.commit();
		const next = { ...this.positions };
		for (const move of targets) next[move.id] = { ...move.position };
		this.positions = next;
		return rejected;
	}

	/** Delete a whole group: members, their edges and the title position. */
	removeGroup(groupId: string): void {
		const members = this.membersOf(groupId);
		if (members.length === 0) return;
		this.removeNodes(members);
		const { [groupTitleId(groupId)]: _dropped, ...rest } = this.positions;
		void _dropped;
		this.positions = rest;
	}

	addNode(node: DisplayNode, position?: GraphPosition): void {
		if (this.nodes.some((entry) => entry.id === node.id)) return;
		this.commit();
		this.nodes = [...this.nodes, { ...node }];
		if (position) {
			this.positions = { ...this.positions, [node.id]: { ...position } };
		}
	}

	removeNodes(ids: string[]): void {
		const doomed = new SvelteSet(ids);
		if (doomed.size === 0) return;
		this.commit();
		this.nodes = this.nodes.filter((node) => !doomed.has(node.id));
		this.edges = this.edges.filter(
			(edge) => !doomed.has(edge.source) && !doomed.has(edge.target),
		);
		this.prunePositions();
		if (this.selectedId !== null && doomed.has(this.selectedId)) {
			this.selectedId = null;
		}
	}

	addEdge(edge: DisplayEdge): void {
		if (this.edges.some((entry) => entry.id === edge.id)) return;
		if (
			!this.nodes.some((node) => node.id === edge.source) ||
			!this.nodes.some((node) => node.id === edge.target)
		) {
			return;
		}
		this.commit();
		this.edges = [...this.edges, { ...edge }];
	}

	/**
	 * Connect two nodes from canvas intent. Duplicate source-target pairs
	 * are ignored so shift-click gestures stay idempotent. Hidden members
	 * and title placeholders are rejected with a reason for a prompt.
	 */
	connect(
		source: string,
		target: string,
		options?: { hiddenIds?: Iterable<string> },
	): string | null {
		if (!source || !target || source === target) return 'Cannot self-connect.';
		const hidden = options?.hiddenIds ? new Set(options.hiddenIds) : null;
		if (hidden?.has(source) || hidden?.has(target)) {
			return 'Hidden group members cannot connect.';
		}
		if (source.startsWith('group:') || target.startsWith('group:')) {
			return 'Group titles cannot connect.';
		}
		if (
			!this.nodes.some((node) => node.id === source) ||
			!this.nodes.some((node) => node.id === target)
		) {
			return 'Unknown node.';
		}
		if (
			this.edges.some(
				(edge) => edge.source === source && edge.target === target,
			)
		) {
			return null;
		}
		let id = `${source}->${target}`;
		let suffix = 2;
		while (this.edges.some((edge) => edge.id === id)) {
			id = `${source}->${target}-${suffix}`;
			suffix += 1;
		}
		this.commit();
		this.edges = [...this.edges, { id, source, target }];
		return null;
	}

	removeEdge(id: string): void {
		if (!this.edges.some((edge) => edge.id === id)) return;
		this.commit();
		this.edges = this.edges.filter((edge) => edge.id !== id);
	}

	undo(): void {
		const previous = this.past.pop();
		if (!previous) return;
		this.future = [...this.future, snapshot(this.nodes, this.edges)];
		this.nodes = previous.nodes;
		this.edges = previous.edges;
		this.prunePositions();
		this.past = [...this.past];
	}

	redo(): void {
		const next = this.future.pop();
		if (!next) return;
		this.past = [...this.past, snapshot(this.nodes, this.edges)];
		this.nodes = next.nodes;
		this.edges = next.edges;
		this.prunePositions();
		this.future = [...this.future];
	}

	/** Backend-shaped definition for the draft save endpoint. */
	toDraftDefinition(workflowId: string, name: string): Record<string, unknown> {
		return {
			id: `${workflowId}-canvas`,
			name,
			nodes: this.nodes.map((node) => ({
				id: node.id,
				node_type: node.kind,
				name: node.label,
			})),
			edges: this.edges.map((edge) => ({
				id: edge.id,
				source_node_id: edge.source,
				target_node_id: edge.target,
				type: draftEdgeType(edge.kind),
				...(edge.label ? { condition: edge.label, label: edge.label } : {}),
			})),
		};
	}

	private commit(): void {
		this.past = [...this.past.slice(-49), snapshot(this.nodes, this.edges)];
		this.future = [];
	}

	private describe(): string {
		return JSON.stringify({ nodes: this.nodes, edges: this.edges });
	}

	private isPositionTarget(id: string): boolean {
		if (this.nodes.some((node) => node.id === id)) return true;
		return this.groupIds().some((groupId) => groupTitleId(groupId) === id);
	}

	private prunePositions(): void {
		const alive = new SvelteSet(this.nodes.map((node) => node.id));
		for (const groupId of this.groupIds()) alive.add(groupTitleId(groupId));
		const pruned: Record<string, GraphPosition> = {};
		for (const [id, position] of Object.entries(this.positions)) {
			if (alive.has(id)) pruned[id] = position;
		}
		this.positions = pruned;
	}
}
