<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { browser } from '$app/environment';
	import type cytoscape from 'cytoscape';
	import type { Core, ElementDefinition, NodeSingular } from 'cytoscape';
	import { columnPositions, layeredPositions } from '$lib/graph/layout';
	import {
		isDashedEdge,
		nodeShape,
		scoreEdgeLabel,
		shortLabel,
		statusHex,
		type DisplayEdge,
		type DisplayNode,
		type GraphLayoutKind,
		type GraphPreset,
	} from '$lib/graph/display-model';
	import { isGroupTitleId, type GroupTitle } from '$lib/graph/group-view';
	import { cn } from '$lib/utils/cn';

	export interface CanvasPosition {
		x: number;
		y: number;
	}

	export interface CanvasMove {
		id: string;
		position: CanvasPosition;
	}

	export interface CanvasContext {
		kind: 'node' | 'edge' | 'blank';
		id: string | null;
		x: number;
		y: number;
	}

	interface Props {
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		preset: GraphPreset;
		layout?: GraphLayoutKind;
		selectedId?: string | null;
		highlightIds?: string[];
		/** Node ids carrying server validation problems. */
		problemIds?: string[];
		/** Nodes currently running; rendered with an emphasized border. */
		pulseIds?: string[];
		/** Nodes on the critical path; rendered with a gold border. */
		criticalIds?: string[];
		/** Slow-node heat tier by node id (1-3); shape never changes. */
		heatTierById?: Record<string, number>;
		/** Decision points; rendered with a distinct dashed outline. */
		decisionIds?: string[];
		/** Position overrides (edit store); unset nodes use the preset layout. */
		positions?: Record<string, CanvasPosition>;
		/** Controlled edit mode: no auto layout, gestures emit intents. */
		editMode?: boolean;
		edgeLabelLimit?: number;
		/** Height class for the canvas container (mini maps use h-56). */
		heightClass?: string;
		class?: string;
		/** Collapsed group ids; members are already folded out by the caller. */
		collapsedIds?: string[];
		/** Pre-cap group titles for collapsed-drag member math. */
		groupTitles?: Record<string, GroupTitle>;
		/** Minimap visibility: auto shows it only on large graphs. */
		minimap?: 'auto' | 'on' | 'off';
		onselect?: (id: string) => void;
		onexpand?: (id: string) => void;
		onboxselect?: (ids: string[]) => void;
		onmovenode?: (id: string, position: CanvasPosition) => void;
		/** Batched move for group drags; undoes in a single step. */
		ongroupmove?: (moves: CanvasMove[]) => void;
		/** Right-click context, positioned in client coordinates. */
		oncontext?: (info: CanvasContext) => void;
		onbackgrounddoubleclick?: (position: CanvasPosition) => void;
		ondeleteedge?: (id: string) => void;
		onconnect?: (source: string, target: string) => void;
	}

	let {
		nodes,
		edges,
		preset,
		layout = 'layered',
		selectedId = null,
		highlightIds = [],
		problemIds = [],
		pulseIds = [],
		criticalIds = [],
		heatTierById = {},
		decisionIds = [],
		positions = undefined,
		editMode = false,
		edgeLabelLimit = 60,
		heightClass = 'h-96',
		class: className = '',
		collapsedIds = [],
		groupTitles = {},
		minimap = 'auto',
		onselect,
		onexpand,
		onboxselect,
		onmovenode,
		ongroupmove,
		oncontext,
		onbackgrounddoubleclick,
		ondeleteedge,
		onconnect,
	}: Props = $props();

	let wrapper: HTMLDivElement | null = $state(null);
	let container: HTMLDivElement | null = $state(null);
	let cy: Core | null = $state(null);
	let ready = $state(false);
	let empty = $derived(nodes.length === 0);
	// Zoomed-out canvases hide non-essential edge labels; the flag only
	// flips when crossing the threshold so zoom gestures stay cheap.
	let zoomedOut = $state(false);

	const highlight = $derived(new Set(highlightIds));
	const problems = $derived(new Set(problemIds));
	const pulses = $derived(new Set(pulseIds));
	const criticals = $derived(new Set(criticalIds));
	const decisions = $derived(new Set(decisionIds));
	const collapsed = $derived(new Set(collapsedIds));
	// Latest props for gesture handlers registered once on mount. Derived
	// values stay current without snapshot effects.
	const selectedSnapshot = $derived(selectedId);
	const editSnapshot = $derived(editMode);
	const connectHandler = $derived(onconnect);
	const selectHandler = $derived(onselect);
	const expandHandler = $derived(onexpand);
	// Snapshots for gesture handlers registered once on mount.
	const positionsSnapshot = $derived(positions);
	const groupTitlesSnapshot = $derived(groupTitles);
	const groupMoveHandler = $derived(ongroupmove);
	const moveHandler = $derived(onmovenode);
	const contextHandler = $derived(oncontext);
	const grabStart = new SvelteMap<string, CanvasPosition>();

	const MINIMAP_AUTO_THRESHOLD = 100;
	const MINI_W = 148;
	const MINI_H = 104;

	interface MiniBounds {
		minX: number;
		minY: number;
		w: number;
		h: number;
	}

	interface MiniOverview {
		items: Array<{ id: string; x: number; y: number }>;
		bounds: MiniBounds;
		view: { x1: number; y1: number; x2: number; y2: number };
	}

	let overview = $state<MiniOverview | null>(null);
	let miniDrag = $state(false);
	let lastMiniRefresh = 0;

	const showMinimap = $derived(
		minimap === 'on' ||
			(minimap === 'auto' && nodes.length >= MINIMAP_AUTO_THRESHOLD),
	);

	/**
	 * Snapshot node dots plus the viewport for the minimap. Structure only:
	 * execution colors stay on the main canvas so hot updates never redraw
	 * the overview.
	 */
	function refreshOverview(): void {
		const core = cy;
		if (!core || !showMinimap) {
			if (overview) overview = null;
			return;
		}
		const items: Array<{ id: string; x: number; y: number }> = [];
		core
			.nodes()
			.filter((node) => !node.isParent())
			.forEach((node) => {
				const position = node.position();
				items.push({ id: node.id(), x: position.x, y: position.y });
			});
		if (items.length === 0) {
			if (overview) overview = null;
			return;
		}
		let minX = Number.POSITIVE_INFINITY;
		let minY = Number.POSITIVE_INFINITY;
		let maxX = Number.NEGATIVE_INFINITY;
		let maxY = Number.NEGATIVE_INFINITY;
		for (const item of items) {
			minX = Math.min(minX, item.x);
			minY = Math.min(minY, item.y);
			maxX = Math.max(maxX, item.x);
			maxY = Math.max(maxY, item.y);
		}
		const pad = 60;
		minX -= pad;
		minY -= pad;
		maxX += pad;
		maxY += pad;
		const extent = core.extent();
		overview = {
			items,
			bounds: {
				minX,
				minY,
				w: maxX - minX || 1,
				h: maxY - minY || 1,
			},
			view: { x1: extent.x1, y1: extent.y1, x2: extent.x2, y2: extent.y2 },
		};
	}

	function requestMiniRefresh(): void {
		const now = Date.now();
		if (now - lastMiniRefresh < 150) return;
		lastMiniRefresh = now;
		refreshOverview();
	}

	function miniXY(x: number, y: number): { x: number; y: number } {
		const bounds = overview?.bounds;
		if (!bounds) return { x: 0, y: 0 };
		return {
			x: ((x - bounds.minX) / bounds.w) * MINI_W,
			y: ((y - bounds.minY) / bounds.h) * MINI_H,
		};
	}

	function miniPoint(event: PointerEvent): { x: number; y: number } {
		const svg = event.currentTarget as SVGSVGElement;
		const rect = svg.getBoundingClientRect();
		const bounds = overview?.bounds;
		if (!bounds || rect.width === 0 || rect.height === 0) return { x: 0, y: 0 };
		return {
			x: bounds.minX + ((event.clientX - rect.left) / rect.width) * bounds.w,
			y: bounds.minY + ((event.clientY - rect.top) / rect.height) * bounds.h,
		};
	}

	function onMiniDown(event: PointerEvent): void {
		if (!overview) return;
		miniDrag = true;
		(event.currentTarget as Element).setPointerCapture(event.pointerId);
		const point = miniPoint(event);
		panTo(point.x, point.y);
	}

	function onMiniMove(event: PointerEvent): void {
		if (!miniDrag || !overview) return;
		const point = miniPoint(event);
		panTo(point.x, point.y);
	}

	function onMiniUp(): void {
		miniDrag = false;
	}

	function roundPosition(position: CanvasPosition): CanvasPosition {
		return { x: Math.round(position.x), y: Math.round(position.y) };
	}

	/**
	 * Moves for a collapsed-title drag: the title plus every member shifted
	 * by the same delta. Members without a stored position are stacked under
	 * the new title position instead of jumping to the origin.
	 */
	function titleDragMoves(titleId: string, next: CanvasPosition): CanvasMove[] {
		const title = groupTitlesSnapshot[titleId];
		const start =
			grabStart.get(titleId) ?? positionsSnapshot?.[titleId] ?? next;
		const delta = { x: next.x - start.x, y: next.y - start.y };
		const moves: CanvasMove[] = [{ id: titleId, position: next }];
		(title?.memberIds ?? []).forEach((memberId, index) => {
			const base = positionsSnapshot?.[memberId];
			moves.push({
				id: memberId,
				position: base
					? { x: base.x + delta.x, y: base.y + delta.y }
					: {
							x: next.x - 120 + (index % 4) * 80,
							y: next.y + 60 + Math.floor(index / 4) * 60,
						},
			});
		});
		return moves;
	}

	interface ConnectSpot {
		id: string;
		x: number;
		y: number;
	}

	interface ConnectDrag {
		source: string;
		sx: number;
		sy: number;
		px: number;
		py: number;
		target: string | null;
	}

	let hotspots = $state<ConnectSpot[]>([]);
	let connectDrag = $state<ConnectDrag | null>(null);

	/** Hotspot dots at node edges; edit mode only. */
	function refreshHotspots(): void {
		const core = cy;
		if (!core || !editSnapshot) {
			if (hotspots.length > 0) hotspots = [];
			return;
		}
		const zoom = core.zoom();
		const spots: ConnectSpot[] = [];
		core
			.nodes()
			.filter((node) => !node.isParent())
			.forEach((node) => {
				const rendered = node.renderedPosition();
				if (!rendered) return;
				spots.push({
					id: node.id(),
					x: rendered.x + (node.width() * zoom) / 2,
					y: rendered.y,
				});
			});
		hotspots = spots;
	}

	function connectValid(source: string, target: string): boolean {
		if (!source || !target || source === target) return false;
		if (target.startsWith('groupbox:')) return false;
		return !edges.some(
			(edge) => edge.source === source && edge.target === target,
		);
	}

	function nodeAtPoint(px: number, py: number): string | null {
		const core = cy;
		if (!core) return null;
		const zoom = core.zoom();
		const hit = core
			.nodes()
			.filter((node) => {
				if (node.isParent()) return false;
				const rendered = node.renderedPosition();
				if (!rendered) return false;
				const halfW = (node.width() * zoom) / 2 + 8;
				const halfH = (node.height() * zoom) / 2 + 8;
				return (
					Math.abs(px - rendered.x) <= halfW &&
					Math.abs(py - rendered.y) <= halfH
				);
			})
			.first();
		return hit.empty() ? null : (hit.id() as string);
	}

	function paintConnectTarget(
		previous: string | null,
		next: string | null,
	): void {
		const core = cy;
		if (!core || previous === next) return;
		if (previous) {
			core.getElementById(previous).removeClass('connect-ok connect-bad');
		}
		if (next && connectDrag) {
			core
				.getElementById(next)
				.addClass(
					connectValid(connectDrag.source, next) ? 'connect-ok' : 'connect-bad',
				);
		}
	}

	function endConnectDrag(commit: boolean): void {
		window.removeEventListener('pointermove', onConnectMove);
		window.removeEventListener('pointerup', onConnectUp);
		window.removeEventListener('keydown', onConnectKey);
		const drag = connectDrag;
		connectDrag = null;
		const core = cy;
		if (core) core.elements().removeClass('connect-ok connect-bad');
		if (
			commit &&
			drag &&
			drag.target &&
			connectValid(drag.source, drag.target)
		) {
			connectHandler?.(drag.source, drag.target);
		}
	}

	function onConnectMove(event: PointerEvent): void {
		const drag = connectDrag;
		if (!drag || !wrapper) return;
		const rect = wrapper.getBoundingClientRect();
		const px = event.clientX - rect.left;
		const py = event.clientY - rect.top;
		const target = nodeAtPoint(px, py);
		paintConnectTarget(drag.target, target);
		connectDrag = { ...drag, px, py, target };
	}

	function onConnectUp(): void {
		endConnectDrag(true);
	}

	function onConnectKey(event: KeyboardEvent): void {
		if (event.key === 'Escape') endConnectDrag(false);
	}

	function startConnect(event: PointerEvent, spot: ConnectSpot): void {
		event.preventDefault();
		event.stopPropagation();
		endConnectDrag(false);
		connectDrag = {
			source: spot.id,
			sx: spot.x,
			sy: spot.y,
			px: spot.x,
			py: spot.y,
			target: null,
		};
		window.addEventListener('pointermove', onConnectMove);
		window.addEventListener('pointerup', onConnectUp);
		window.addEventListener('keydown', onConnectKey);
	}

	function presetPositions(): Record<string, { x: number; y: number }> {
		if (layout === 'columns') {
			return Object.fromEntries(columnPositions(nodes));
		}
		if (layout === 'layered') {
			return Object.fromEntries(layeredPositions(nodes, edges));
		}
		return {};
	}

	function elementDefs(): ElementDefinition[] {
		const computed = presetPositions();
		// Expanded groups render as compound parent boxes; collapsed groups
		// arrive pre-folded as title nodes with their members excluded.
		const boxIds = [
			...new Set(
				nodes.flatMap((node) =>
					node.groupId &&
					!collapsed.has(node.groupId) &&
					!isGroupTitleId(node.id)
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
				color: statusHex(node.status),
				...(node.groupId && boxIds.includes(node.groupId)
					? { parent: `groupbox:${node.groupId}` }
					: {}),
			},
			position: positions?.[node.id] ?? computed[node.id],
			classes: [
				selectedId === node.id ? 'selected' : '',
				isGroupTitleId(node.id) ? 'group-title' : '',
				problems.has(node.id) ? 'problem' : '',
				pulses.has(node.id) ? 'running' : '',
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
		for (const edge of edges) {
			const touchesSelection =
				selectedId !== null &&
				(edge.source === selectedId || edge.target === selectedId);
			defs.push({
				group: 'edges' as const,
				data: {
					id: edge.id,
					source: edge.source,
					target: edge.target,
					label: edgeLabelFor(edge, touchesSelection),
					lineStyle: isDashedEdge(edge.kind, edge.taken ?? true)
						? 'dashed'
						: 'solid',
					color:
						statusHex(edge.status) === '#71717a'
							? '#71717a'
							: statusHex(edge.status),
				},
			});
		}
		return defs;
	}

	/**
	 * Label visibility for one edge. Selection-adjacent labels always show.
	 * Zoomed-out canvases hide the rest. Over-budget graphs rank the rest
	 * by importance and keep only the top slice.
	 */
	function edgeLabelFor(edge: DisplayEdge, touchesSelection: boolean): string {
		const label = edge.label ?? '';
		if (!label) return '';
		if (touchesSelection) return label;
		if (zoomedOut) return '';
		if (edges.length <= edgeLabelLimit) return label;
		return rankedEdgeIds().has(edge.id) ? label : '';
	}

	const rankedCache = new SvelteMap<string, Set<string>>();

	function rankedEdgeIds(): Set<string> {
		const key = `${edges.length}:${edgeLabelLimit}:${selectedId ?? ''}:${[...highlight].sort().join(',')}`;
		const cached = rankedCache.get(key);
		if (cached) return cached;
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
		const ranked = new Set(scored);
		rankedCache.clear();
		rankedCache.set(key, ranked);
		return ranked;
	}

	function layoutOptions(): Record<string, unknown> {
		switch (layout) {
			case 'columns':
			case 'layered':
				return { name: 'preset', padding: 30, fit: true };
			case 'force':
				return {
					name: 'cose',
					padding: 30,
					animate: false,
					randomize: true,
					fit: true,
				};
			case 'grid':
				return { name: 'grid', padding: 30, fit: true, avoidOverlap: true };
			default:
				return { name: 'preset', padding: 30, fit: true };
		}
	}

	function syncElements(): void {
		if (!cy) return;
		const defs = elementDefs();
		const wanted = new SvelteMap(
			defs.map((def) => [def.data.id as string, def]),
		);
		cy.batch(() => {
			if (!cy) return;
			cy.elements().forEach((element) => {
				const id = element.id();
				const def = wanted.get(id);
				if (!def) {
					element.remove();
					return;
				}
				element.data({ ...element.data(), ...def.data });
				// Compound parents are sized by their children; only
				// childless nodes take explicit positions. Parent links only
				// change across remove/add cycles, so presence is enough.
				if (element.isNode() && element.isChildless()) {
					const parent = (def.data as { parent?: string }).parent;
					const hasParent = !element.parent().empty();
					if (parent && !hasParent) {
						element.move({ parent });
					} else if (!parent && hasParent) {
						element.move({ parent: null });
					}
				}
				const classes = (def.classes ?? '') as string;
				element.classes(classes);
				if (def.position && element.isNode() && element.isChildless()) {
					element.position(def.position);
				}
				wanted.delete(id);
			});
			if (wanted.size > 0) {
				cy.add([...wanted.values()]);
			}
		});
	}

	function runLayout(): void {
		if (!cy) return;
		cy.layout(layoutOptions() as never).run();
	}

	// Set when relayout runs in edit mode; consumed on layoutstop so the
	// rearranged positions persist as a single undoable store update.
	let pendingLayoutCommit = false;

	export function zoomIn(): void {
		if (!cy) return;
		const extent = cy.extent();
		cy.zoom({
			level: Math.min(cy.zoom() * 1.25, 4),
			renderedPosition: {
				x: (extent.x1 + extent.x2) / 2,
				y: (extent.y1 + extent.y2) / 2,
			},
		});
	}

	export function zoomOut(): void {
		if (!cy) return;
		const extent = cy.extent();
		cy.zoom({
			level: Math.max(cy.zoom() / 1.25, 0.2),
			renderedPosition: {
				x: (extent.x1 + extent.x2) / 2,
				y: (extent.y1 + extent.y2) / 2,
			},
		});
	}

	export function fit(): void {
		cy?.fit(undefined, 30);
	}

	export function zoomTo(id: string): void {
		const core = cy;
		if (!core) return;
		const target = core.getElementById(id);
		if (target.empty() || !target.isNode()) return;
		void core.animate(
			{ center: { eles: target }, zoom: Math.max(core.zoom(), 1.2) },
			{ duration: 250 },
		);
	}

	export function fitTo(ids: string[]): void {
		const core = cy;
		if (!core || ids.length === 0) return;
		const eles = core.collection();
		for (const id of ids) {
			const found = core.getElementById(id);
			if (!found.empty()) eles.merge(found);
		}
		if (eles.empty()) return;
		core.fit(eles, 40);
	}

	export function panTo(x: number, y: number): void {
		if (!cy || !container) return;
		const zoom = cy.zoom();
		void cy.animate(
			{
				pan: {
					x: container.clientWidth / 2 - x * zoom,
					y: container.clientHeight / 2 - y * zoom,
				},
			},
			{ duration: 200 },
		);
	}

	/** Select every non-box node (shortcut delete workflow). */
	export function selectAll(): void {
		cy?.nodes()
			.filter((node) => !node.isParent())
			.select();
	}

	export function relayout(): void {
		// In edit mode the rearranged positions flow back to the store as
		// one history entry; elsewhere the layout is purely visual.
		pendingLayoutCommit = editSnapshot && !!groupMoveHandler;
		runLayout();
	}

	/** Ids currently selected on the canvas (edit toolbar delete). */
	export function selectedNodeIds(): string[] {
		const boxed =
			cy
				?.nodes(':selected')
				.filter((node) => !node.isParent())
				.map((node) => node.id() as string) ?? [];
		if (boxed.length > 0) return boxed;
		return selectedId ? [selectedId] : [];
	}

	export function exportPng(): boolean {
		if (!cy) return false;
		try {
			const blob = cy.png({ output: 'blob', full: true, scale: 2 }) as Blob;
			const url = URL.createObjectURL(blob);
			const anchor = document.createElement('a');
			anchor.href = url;
			anchor.download = 'graph.png';
			anchor.click();
			setTimeout(() => URL.revokeObjectURL(url), 1000);
			return true;
		} catch {
			return false;
		}
	}

	let lastTap: { id: string; at: number } | null = null;

	onMount(() => {
		if (!browser || !container) return;
		let cancelled = false;
		let instance: Core | null = null;
		void (async () => {
			const { default: cytoscape } = await import('cytoscape');
			if (cancelled || !container) return;
			instance = cytoscape({
				container,
				elements: elementDefs(),
				layout: layoutOptions() as never,
				minZoom: 0.2,
				maxZoom: 4,
				boxSelectionEnabled: true,
				// Data mappers (`data(field)`) are core cytoscape behavior but
				// postdate the shipped stylesheet types, hence the cast.
				style: [
					{
						selector: 'node',
						style: {
							shape: 'data(shape)',
							width: 120,
							height: 40,
							'background-color': '#18181b',
							'background-opacity': 0.9,
							'border-width': 1.5,
							'border-color': 'data(color)',
							label: 'data(label)',
							color: '#e4e4e7',
							'font-size': 10,
							'text-valign': 'center',
							'text-halign': 'center',
							'text-wrap': 'ellipsis',
							'text-max-width': 104,
						},
					},
					{
						selector: 'node.selected',
						style: {
							'border-width': 3,
							'border-color': '#fafafa',
						},
					},
					{
						selector: 'node.problem',
						style: {
							'border-width': 2.5,
							'border-color': '#dc2626',
							'border-style': 'dashed',
						},
					},
					{
						selector: 'node.running',
						style: {
							'border-width': 3,
							'border-color': '#2563eb',
						},
					},
					{
						selector: 'node.critical',
						style: {
							'border-width': 2.5,
							'border-color': '#f59e0b',
						},
					},
					{
						selector: 'node.decision',
						style: {
							'border-width': 2.5,
							'border-color': '#7c3aed',
							'border-style': 'dashed',
						},
					},
					{
						selector: 'node.heat-1',
						style: {
							'border-width': 2,
							'border-color': '#fbbf24',
						},
					},
					{
						selector: 'node.heat-2',
						style: {
							'border-width': 2.5,
							'border-color': '#f97316',
						},
					},
					{
						selector: 'node.heat-3',
						style: {
							'border-width': 3,
							'border-color': '#ea580c',
						},
					},
					{
						selector: 'node.highlighted',
						style: {
							'border-width': 3,
							'border-color': '#f59e0b',
						},
					},
					{
						selector: 'node.dimmed',
						style: { opacity: 0.3 },
					},
					{
						selector: 'edge',
						style: {
							width: 1.5,
							'line-color': 'data(color)',
							'line-style': 'data(lineStyle)',
							'target-arrow-shape': 'triangle',
							'target-arrow-color': 'data(color)',
							'curve-style': 'bezier',
							label: 'data(label)',
							color: '#a1a1aa',
							'font-size': 9,
							'edge-text-rotation': 'autorotate',
							'text-background-color': '#18181b',
							'text-background-opacity': 0.7,
							'text-background-padding': 2,
						},
					},
					{
						selector: 'node.group-title',
						style: {
							width: 160,
							'border-width': 2,
							'border-style': 'dashed',
							'border-color': '#f59e0b',
						},
					},
					{
						selector: 'node.group-box',
						style: {
							'background-opacity': 0.15,
							'background-color': '#52525b',
							'border-width': 1,
							'border-style': 'dashed',
							'border-color': '#a1a1aa',
							label: 'data(label)',
							color: '#d4d4d8',
							'font-size': 10,
							'text-valign': 'top',
							'text-halign': 'center',
							padding: '14px',
						},
					},
					{
						selector: 'node.connect-ok',
						style: {
							'border-width': 3,
							'border-color': '#16a34a',
						},
					},
					{
						selector: 'node.connect-bad',
						style: { opacity: 0.45 },
					},
					{
						selector: 'edge.dimmed',
						style: { opacity: 0.25 },
					},
				] as unknown as cytoscape.StylesheetJson,
			});
			instance.on('tap', 'node', (event) => {
				const id = event.target.id() as string;
				if (editSnapshot) {
					const original = (event as unknown as { originalEvent?: MouseEvent })
						.originalEvent;
					const from = selectedSnapshot;
					if (original?.shiftKey && from && from !== id && connectHandler) {
						connectHandler(from, id);
						return;
					}
				}
				const now = Date.now();
				if (lastTap && lastTap.id === id && now - lastTap.at < 350) {
					lastTap = null;
					expandHandler?.(id);
					return;
				}
				lastTap = { id, at: now };
				selectHandler?.(id);
			});
			// Edit intents: the canvas never mutates business state itself,
			// it only reports what the user did back to the edit store.
			instance.on('tap', 'edge', (event) => {
				if (!editMode) return;
				ondeleteedge?.(event.target.id() as string);
			});
			instance.on('grab', 'node', (event) => {
				const target = event.target;
				grabStart.set(
					target.id() as string,
					roundPosition(target.position() as CanvasPosition),
				);
			});
			instance.on('dragfree', 'node', (event) => {
				requestMiniRefresh();
				refreshHotspots();
				if (!editMode) return;
				const target = event.target;
				const id = target.id() as string;
				const position = roundPosition(target.position() as CanvasPosition);
				if (isGroupTitleId(id)) {
					groupMoveHandler?.(titleDragMoves(id, position));
					return;
				}
				const parent = target.parent();
				if (id.startsWith('groupbox:') || !parent.empty()) {
					// A group moves as one unit; report every member so the
					// store can persist the drag as a single history entry.
					const siblings = id.startsWith('groupbox:')
						? target.children()
						: parent.children();
					const moves: CanvasMove[] = [];
					siblings.forEach((child: NodeSingular) => {
						moves.push({
							id: child.id() as string,
							position: roundPosition(child.position() as CanvasPosition),
						});
					});
					if (moves.length > 0) {
						groupMoveHandler?.(moves);
						return;
					}
				}
				moveHandler?.(id, position);
			});
			instance.on('dblclick', (event) => {
				if (!editMode) return;
				if (event.target !== instance) return;
				const position = (event as unknown as { position: CanvasPosition })
					.position;
				onbackgrounddoubleclick?.({
					x: Math.round(position.x),
					y: Math.round(position.y),
				});
			});
			const core = instance;
			core.on('zoom', () => {
				const out = core.zoom() < 0.6;
				if (out !== zoomedOut) zoomedOut = out;
				requestMiniRefresh();
				refreshHotspots();
			});
			core.on('pan', () => {
				requestMiniRefresh();
				refreshHotspots();
			});
			core.on('layoutstop', () => {
				refreshOverview();
				refreshHotspots();
				if (pendingLayoutCommit) {
					pendingLayoutCommit = false;
					const moves: CanvasMove[] = [];
					core
						.nodes()
						.filter((node) => !node.isParent())
						.forEach((node) => {
							moves.push({
								id: node.id() as string,
								position: roundPosition(node.position() as CanvasPosition),
							});
						});
					if (moves.length > 0) groupMoveHandler?.(moves);
				}
			});
			core.on('cxttap', (event) => {
				const original = (event as unknown as { originalEvent?: MouseEvent })
					.originalEvent;
				const info = {
					x: original?.clientX ?? 0,
					y: original?.clientY ?? 0,
				};
				const target = event.target as unknown as {
					id?: () => unknown;
					isEdge?: () => boolean;
				};
				if (target === instance || typeof target?.id !== 'function') {
					contextHandler?.({ kind: 'blank', id: null, ...info });
					return;
				}
				const id = target.id() as string;
				if (id.startsWith('groupbox:')) {
					contextHandler?.({ kind: 'blank', id: null, ...info });
					return;
				}
				contextHandler?.({
					kind: target.isEdge?.() ? 'edge' : 'node',
					id,
					...info,
				});
			});
			core.on('boxend', () => {
				const ids = core.$('node:selected').map((node) => node.id());
				if (ids.length > 0) onboxselect?.(ids);
			});
			cy = instance;
			ready = true;
		})();
		return () => {
			cancelled = true;
			instance?.destroy();
			cy = null;
		};
	});

	$effect(() => {
		if (!ready) return;
		void nodes;
		void edges;
		void selectedId;
		void highlightIds;
		void problemIds;
		void pulseIds;
		void criticalIds;
		void heatTierById;
		void decisionIds;
		void positions;
		void collapsedIds;
		void groupTitles;
		void preset;
		void edgeLabelLimit;
		void zoomedOut;
		void showMinimap;
		syncElements();
		refreshOverview();
		refreshHotspots();
	});

	$effect(() => {
		if (!ready || !cy) return;
		void editMode;
		if (editMode) {
			cy.autoungrabify(false);
			cy.nodes().grabify();
		} else {
			cy.nodes().ungrabify();
			cy.autoungrabify(true);
		}
		if (!editMode && connectDrag) endConnectDrag(false);
		refreshHotspots();
	});

	$effect(() => {
		if (!ready || editMode) return;
		void layout;
		runLayout();
	});
</script>

<div
	bind:this={wrapper}
	role="application"
	aria-label="Graph canvas workspace"
	class={cn(
		'relative overflow-hidden rounded-lg border border-border bg-muted/30',
		className,
	)}
	oncontextmenu={(event) => event.preventDefault()}
>
	<div
		bind:this={container}
		class={`w-full ${heightClass}`}
		role="img"
		aria-label="Graph canvas"
	></div>
	{#if editMode && !connectDrag}
		{#each hotspots as spot (spot.id)}
			<button
				type="button"
				aria-label="Connect from {spot.id}"
				title="Drag to connect from {spot.id}"
				class="absolute z-10 h-3 w-3 -translate-x-1/2 -translate-y-1/2 cursor-crosshair rounded-full border border-info bg-card hover:bg-info"
				style:left={`${spot.x}px`}
				style:top={`${spot.y}px`}
				onpointerdown={(event) => startConnect(event, spot)}
			></button>
		{/each}
	{/if}
	{#if connectDrag}
		<svg
			class="pointer-events-none absolute inset-0 z-10 h-full w-full"
			role="presentation"
		>
			<line
				x1={connectDrag.sx}
				y1={connectDrag.sy}
				x2={connectDrag.px}
				y2={connectDrag.py}
				class={connectDrag.target &&
				connectValid(connectDrag.source, connectDrag.target)
					? 'stroke-success'
					: 'stroke-destructive'}
				stroke-width="2"
				stroke-dasharray="5 4"
			/>
		</svg>
	{/if}
	{#if !ready}
		<div
			class="pointer-events-none absolute inset-0 flex items-center justify-center text-caption text-muted-foreground"
		>
			Loading graph renderer…
		</div>
	{:else if empty}
		<div
			class="pointer-events-none absolute inset-0 flex items-center justify-center text-caption text-muted-foreground"
		>
			No nodes to display.
		</div>
	{/if}
	<div
		class="pointer-events-none absolute right-2 bottom-2 rounded-md border border-border bg-card/90 px-2 py-1 text-micro text-muted-foreground"
	>
		{nodes.length} nodes · {edges.length} edges
	</div>
	{#if showMinimap && overview}
		{@const topLeft = miniXY(overview.view.x1, overview.view.y1)}
		{@const bottomRight = miniXY(overview.view.x2, overview.view.y2)}
		<div
			class="absolute bottom-2 left-2 rounded-md border border-border bg-card/90 p-1"
		>
			<svg
				width={MINI_W}
				height={MINI_H}
				role="img"
				aria-label="Graph minimap"
				class="block cursor-crosshair touch-none"
				onpointerdown={onMiniDown}
				onpointermove={onMiniMove}
				onpointerup={onMiniUp}
				onpointercancel={onMiniUp}
			>
				{#each overview.items as item (item.id)}
					{@const point = miniXY(item.x, item.y)}
					<rect
						x={point.x - 1.5}
						y={point.y - 1.5}
						width="3"
						height="3"
						rx="0.75"
						class="fill-muted-foreground"
					/>
				{/each}
				<rect
					x={Math.min(topLeft.x, bottomRight.x)}
					y={Math.min(topLeft.y, bottomRight.y)}
					width={Math.abs(bottomRight.x - topLeft.x)}
					height={Math.abs(bottomRight.y - topLeft.y)}
					class="fill-transparent stroke-info"
					stroke-width="1.5"
				/>
			</svg>
		</div>
	{/if}
</div>
