import {
	GRAPH_NODE_CAP,
	rankTone,
	statusForTone,
	toneForStatus,
	type DisplayEdge,
	type DisplayNode,
	type ExecutionTone,
} from './display-model';

/** Frontend-only group definition. Membership comes from `DisplayNode.groupId`. */
export interface GroupDef {
	id: string;
	label: string;
}

export interface GroupTitle {
	groupId: string;
	label: string;
	memberIds: string[];
	/** Aggregated display status of the members; drives title emphasis. */
	status?: string;
}

export interface GroupViewOptions {
	/** Explicit group labels; member labels win when present. */
	labels?: Record<string, string>;
	/** Status override by node id; falls back to the member status. */
	statusById?: Record<string, string | undefined>;
}

/** Id of the placeholder node representing a collapsed group. */
export function groupTitleId(groupId: string): string {
	return `group:${groupId}`;
}

/** Whether a node id is a collapsed-group placeholder. */
export function isGroupTitleId(id: string): boolean {
	return id.startsWith('group:');
}

/** Group id behind a title node id. */
export function groupIdFromTitle(titleId: string): string {
	return titleId.slice('group:'.length);
}

export interface GroupView {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
	hiddenIds: Set<string>;
	titleIds: Set<string>;
	titles: Record<string, GroupTitle>;
	canonicals: Map<string, Array<{ source: string; target: string }>>;
}

/** Distinct groups present in the node set, in stable id order. */
export function deriveGroups(nodes: DisplayNode[]): GroupDef[] {
	const labels = new Map<string, string>();
	for (const node of nodes) {
		if (!node.groupId) continue;
		if (!labels.has(node.groupId)) {
			const trimmed = (node.groupLabel ?? '').trim();
			labels.set(node.groupId, trimmed ? trimmed : node.groupId);
		}
		const current = labels.get(node.groupId);
		if (current === node.groupId) {
			const trimmed = (node.groupLabel ?? '').trim();
			if (trimmed) labels.set(node.groupId, trimmed);
		}
	}
	return [...labels.entries()]
		.sort(([a], [b]) => (a < b ? -1 : 1))
		.map(([id, label]) => ({ id, label }));
}

/** Aggregated display status for a member set using the shared tone order. */
export function aggregateGroupStatus(
	statuses: Array<string | undefined>,
): string | undefined {
	let tone: ExecutionTone = 'neutral';
	for (const status of statuses) {
		tone = rankTone(tone, toneForStatus(status));
	}
	return statusForTone(tone);
}

/**
 * Fold collapsed groups out of the render set. Members of a collapsed group
 * are hidden behind one title node; edges crossing the boundary are
 * redirected to the title and merged when they share endpoints, with the
 * real endpoints kept in `canonicals`. Boundary edge status is the highest
 * priority tone of the merged real edges.
 */
export function buildGroupView(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	collapsed: Set<string>,
	options?: GroupViewOptions,
): GroupView {
	const empty: GroupView = {
		nodes: [...nodes],
		edges: [...edges],
		hiddenIds: new Set(),
		titleIds: new Set(),
		titles: {},
		canonicals: new Map(),
	};
	if (collapsed.size === 0) return empty;
	const members = new Map<string, DisplayNode[]>();
	for (const node of nodes) {
		if (node.groupId && collapsed.has(node.groupId)) {
			const list = members.get(node.groupId) ?? [];
			list.push(node);
			members.set(node.groupId, list);
		}
	}
	if (members.size === 0) return empty;
	const hiddenIds = new Set<string>();
	const titleIds = new Set<string>();
	const titles: Record<string, GroupTitle> = {};
	const visible: DisplayNode[] = [];
	for (const node of nodes) {
		if (node.groupId && collapsed.has(node.groupId)) {
			hiddenIds.add(node.id);
		} else {
			visible.push(node);
		}
	}
	for (const [groupId, list] of [...members.entries()].sort(([a], [b]) =>
		a < b ? -1 : 1,
	)) {
		const titleId = groupTitleId(groupId);
		titleIds.add(titleId);
		const memberLabel = list
			.map((node) => (node.groupLabel ?? '').trim())
			.find((label) => label.length > 0);
		const label =
			(options?.labels?.[groupId] ?? '').trim() || memberLabel || groupId;
		const status = aggregateGroupStatus(
			list.map((node) => options?.statusById?.[node.id] ?? node.status),
		);
		titles[titleId] = {
			groupId,
			label,
			memberIds: list.map((node) => node.id).sort(),
			...(status ? { status } : {}),
		};
		visible.push({
			id: titleId,
			label,
			kind: 'group',
			...(status ? { status } : {}),
			groupId,
		});
	}
	const redirect = (id: string, groups: Map<string, string>): string =>
		groups.get(id) ?? id;
	const endpointGroup = new Map<string, string>();
	for (const [groupId, list] of members) {
		for (const node of list) endpointGroup.set(node.id, groupTitleId(groupId));
	}
	const merged = new Map<
		string,
		{ edge: DisplayEdge; canonicals: Array<{ source: string; target: string }> }
	>();
	for (const edge of edges) {
		const sourceHidden = hiddenIds.has(edge.source);
		const targetHidden = hiddenIds.has(edge.target);
		if (sourceHidden && targetHidden) {
			const sourceTitle = endpointGroup.get(edge.source);
			const targetTitle = endpointGroup.get(edge.target);
			if (sourceTitle === targetTitle) continue;
		}
		if (!sourceHidden && !targetHidden) {
			merged.set(edge.id, {
				edge: { ...edge },
				canonicals: [{ source: edge.source, target: edge.target }],
			});
			continue;
		}
		const source = redirect(edge.source, endpointGroup);
		const target = redirect(edge.target, endpointGroup);
		const key = `${source}->${target}`;
		const existing = merged.get(key);
		const endpoint = { source: edge.source, target: edge.target };
		if (existing) {
			existing.canonicals.push(endpoint);
			existing.edge = mergeBoundaryEdge(existing.edge, edge);
		} else {
			merged.set(key, {
				edge: {
					id: key,
					source,
					target,
					label: edge.label,
					kind: edge.kind,
					status: edge.status,
				},
				canonicals: [endpoint],
			});
		}
	}
	const outEdges: DisplayEdge[] = [];
	const canonicals = new Map<
		string,
		Array<{ source: string; target: string }>
	>();
	let suffix = 2;
	for (const { edge: mergedEdge, canonicals: endpoints } of merged.values()) {
		let id = mergedEdge.id;
		while (outEdges.some((entry) => entry.id === id)) {
			id = `${mergedEdge.id}-${suffix}`;
			suffix += 1;
		}
		const finalEdge =
			endpoints.length > 1
				? { ...mergedEdge, id, label: `${endpoints.length} links` }
				: { ...mergedEdge, id };
		outEdges.push(finalEdge);
		canonicals.set(id, endpoints);
	}
	return {
		nodes: visible,
		edges: outEdges,
		hiddenIds,
		titleIds,
		titles,
		canonicals,
	};
}

function mergeBoundaryEdge(into: DisplayEdge, edge: DisplayEdge): DisplayEdge {
	const tone = rankTone(toneForStatus(into.status), toneForStatus(edge.status));
	return {
		...into,
		kind: into.kind === edge.kind ? into.kind : undefined,
		status: statusForTone(tone),
	};
}

/** Highest-priority tone of a member set; drives the group title color. */
export function aggregateGroupTone(tones: ExecutionTone[]): ExecutionTone {
	let tone: ExecutionTone = 'neutral';
	for (const next of tones) tone = rankTone(tone, next);
	return tone;
}

/**
 * Title position as the centroid of member positions, so a collapsed group
 * sits where its members were. Members without a position are ignored.
 */
export function titlePosition(
	memberIds: string[],
	positions: Record<string, { x: number; y: number }>,
): { x: number; y: number } | undefined {
	let x = 0;
	let y = 0;
	let count = 0;
	for (const id of memberIds) {
		const position = positions[id];
		if (!position) continue;
		x += position.x;
		y += position.y;
		count += 1;
	}
	if (count === 0) return undefined;
	return { x: Math.round(x / count), y: Math.round(y / count) };
}

export interface FoldedGroups {
	view: GroupView;
	collapsed: Set<string>;
	auto: string[];
}

/**
 * Fold the largest non-protected groups until the visible node count fits
 * the cap. Protected groups (selection, live and critical members) never
 * auto-fold. Pure function; the caller owns the manual collapsed set.
 */
export function foldForCap(
	nodes: DisplayNode[],
	edges: DisplayEdge[],
	manual: Set<string>,
	protectedGroups: Set<string>,
	cap: number = GRAPH_NODE_CAP,
	options?: GroupViewOptions,
): FoldedGroups {
	const collapsed = new Set(manual);
	let view = buildGroupView(nodes, edges, collapsed, options);
	if (view.nodes.length <= cap) return { view, collapsed, auto: [] };
	const sizes = new Map<string, number>();
	for (const node of nodes) {
		if (!node.groupId || collapsed.has(node.groupId)) continue;
		sizes.set(node.groupId, (sizes.get(node.groupId) ?? 0) + 1);
	}
	const ordered = [...sizes.entries()]
		.sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
		.map(([id]) => id)
		.filter((id) => !protectedGroups.has(id));
	const auto: string[] = [];
	for (const id of ordered) {
		collapsed.add(id);
		auto.push(id);
		view = buildGroupView(nodes, edges, collapsed, options);
		if (view.nodes.length <= cap) break;
	}
	return { view, collapsed, auto };
}
