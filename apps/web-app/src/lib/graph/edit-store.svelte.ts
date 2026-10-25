import type { DisplayEdge, DisplayNode } from './display-model';
import { backendEdgeType, connectByPort } from './display-model';
import { canvasDraftId } from './canvas-model';
import type { WorkflowDefinitionDoc } from '#lib/types/models';
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
	positions: Record<string, GraphPosition>;
}

function snapshot(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	positions: Record<string, GraphPosition>,
): HistoryEntry {
	return {
		nodes: nodes.map((node) => ({ ...node })),
		edges: edges.map((edge) => ({ ...edge })),
		positions: Object.fromEntries(
			Object.entries(positions).map(([id, position]) => [id, { ...position }]),
		),
	};
}

/**
 * Controlled edit state for the graph canvas. The canvas only emits intents
 * (move, add, delete); every mutation lands here, and the canvas re-renders
 * from this single source. Undo and redo replay whole snapshots so grouped
 * drag gestures collapse to one history entry.
 *
 * Topology (nodes/edges) is draft state: `dirty` compares it against the
 * last draft save. Positions are layout state persisted through the layout
 * endpoint, never the draft payload: moves commit history entries (so they
 * undo/redo) and mark `layoutDirty`, but leave `dirty` untouched.
 */
export class GraphEditStore {
	nodes = $state<DisplayNode[]>([]);
	edges = $state<DisplayEdge[]>([]);
	positions = $state<Record<string, GraphPosition>>({});
	selectedId = $state<string | null>(null);

	private baseline = $state('');
	private layoutBaseline = $state('');
	private past = $state<HistoryEntry[]>([]);
	private future = $state<HistoryEntry[]>([]);

	/**
	 * Topology drift (nodes/edges) against the last draft save. Canvas
	 * positions are layout state persisted through the layout endpoint, not
	 * the draft payload, so they never mark the draft dirty.
	 */
	get dirty(): boolean {
		return this.describe() !== this.baseline;
	}

	/**
	 * Layout drift (canvas positions) against the last layout baseline.
	 * Moves are undoable but do not dirty the draft; the canvas persists
	 * them through the layout endpoint instead.
	 */
	get layoutDirty(): boolean {
		return this.describePositions() !== this.layoutBaseline;
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
		this.layoutBaseline = this.describePositions();
		this.selectedId = null;
	}

	/** Record the topology as draft-clean; layout drift is kept as-is. */
	markClean(): void {
		this.baseline = this.describe();
	}

	/** Record the positions as layout-clean after a layout save or reset. */
	markLayoutClean(): void {
		this.layoutBaseline = this.describePositions();
	}

	applyMove(
		id: string,
		position: GraphPosition,
		options?: { hiddenIds?: Iterable<string> },
	): string | null {
		if (!this.nodes.some((node) => node.id === id)) return 'Unknown node.';
		if (options?.hiddenIds && new SvelteSet(options.hiddenIds).has(id)) {
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
	applyMoves(moves: GraphMove[], options?: { hiddenIds?: Iterable<string> }): string[] {
		const hidden = options?.hiddenIds ? new SvelteSet(options.hiddenIds) : null;
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
		this.edges = this.edges.filter((edge) => !doomed.has(edge.source) && !doomed.has(edge.target));
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
		const hidden = options?.hiddenIds ? new SvelteSet(options.hiddenIds) : null;
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
		if (this.edges.some((edge) => edge.source === source && edge.target === target)) {
			return null;
		}
		// Port-type validation mirrors the canvas pre-check and the backend
		// boundary rules: entry nodes accept no input, exit nodes emit no
		// output. The store enforces it so programmatic connects cannot
		// bypass the canvas hotspot checks.
		const sourceNode = this.nodes.find((node) => node.id === source);
		const targetNode = this.nodes.find((node) => node.id === target);
		const portReason = connectByPort({
			sourceKind: sourceNode?.kind ?? '',
			targetKind: targetNode?.kind ?? '',
			sourcePort: null,
			targetPort: 'input',
		});
		if (portReason) return portReason;
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
		this.future = [...this.future, snapshot(this.nodes, this.edges, this.positions)];
		this.nodes = previous.nodes;
		this.edges = previous.edges;
		this.positions = previous.positions;
		this.prunePositions();
		this.past = [...this.past];
	}

	redo(): void {
		const next = this.future.pop();
		if (!next) return;
		this.past = [...this.past, snapshot(this.nodes, this.edges, this.positions)];
		this.nodes = next.nodes;
		this.edges = next.edges;
		this.positions = next.positions;
		this.prunePositions();
		this.future = [...this.future];
	}

	/** Update topology attributes of one node with undo support. */
	updateNode(id: string, patch: Partial<Pick<DisplayNode, 'label' | 'kind'>>): void {
		if (!this.nodes.some((node) => node.id === id)) return;
		this.commit();
		this.nodes = this.nodes.map((node) => (node.id === id ? { ...node, ...patch } : node));
	}

	/**
	 * Backend-shaped definition for the draft save endpoint.
	 *
	 * The draft carries the whole definition, not just the edited graph: the
	 * description, the configuration and the metadata are the definition's own
	 * properties, and promoting the draft must not drop them. Only the graph
	 * is this edit's content.
	 */
	toDraftDefinition(
		workflowId: string,
		name: string,
		base: WorkflowDefinitionDoc,
	): Record<string, unknown> {
		return {
			...base,
			id: canvasDraftId(workflowId),
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
				type: backendEdgeType(edge.kind),
				...(edge.label ? { condition: edge.label } : {}),
			})),
		};
	}

	private commit(): void {
		this.past = [...this.past.slice(-49), snapshot(this.nodes, this.edges, this.positions)];
		this.future = [];
	}

	private describe(): string {
		return JSON.stringify({ nodes: this.nodes, edges: this.edges });
	}

	private describePositions(): string {
		const ids = Object.keys(this.positions).sort();
		return JSON.stringify(Object.fromEntries(ids.map((id) => [id, this.positions[id]])));
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
