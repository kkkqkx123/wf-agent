<script lang="ts" module>
	export type * from '$lib/graph/canvas-model';
</script>

<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { browser } from '$app/environment';
	import type { Core, ElementDefinition, NodeSingular } from 'cytoscape';
	import { pushOverlapped } from '$lib/graph/layout';
	import type {
		DisplayEdge,
		DisplayNode,
		GraphLayoutKind,
		GraphPreset,
	} from '$lib/graph/display-model';
	import { buildElementDefs, rankEdgeIds } from '$lib/graph/canvas-elements';
	import {
		canvasLayoutOptions,
		presetPositions as presetPositionsFor,
	} from '$lib/graph/canvas-layout';
	import {
		describeConnectRejection,
		isConnectValid,
	} from '$lib/graph/canvas-connect';
	import { roundPosition, titleDragMoves } from '$lib/graph/canvas-drag';
	import {
		computeMiniOverview,
		MINIMAP_AUTO_THRESHOLD,
		MINI_H,
		MINI_W,
		miniPointerToWorld,
		projectToMini,
	} from '$lib/graph/canvas-minimap';
	import { HoverTip } from '$lib/graph/canvas-tooltip.svelte';
	import type {
		CanvasContext,
		CanvasMove,
		CanvasPosition,
		ConnectDrag,
		ConnectSpot,
		MiniOverview,
	} from '$lib/graph/canvas-model';
	import { CANVAS_STYLESHEET } from '$lib/graph/canvas-style';
	import { isGroupTitleId, type GroupTitle } from '$lib/graph/group-view';
	import { cn } from '$lib/utils/cn';

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
		/** Failed nodes; rendered with a solid failure border. The running
		 * pulse keeps precedence on the current node. */
		failedIds?: string[];
		/** Nodes on the critical path; rendered with a gold border. */
		criticalIds?: string[];
		/** Slow-node heat tier by node id (1-3); shape never changes. */
		heatTierById?: Record<string, number>;
		/** Decision points; rendered with a distinct dashed outline. */
		decisionIds?: string[];
		/** Hover text by node id (slow-node duration, decision branches).
		 * Same information the selected card shows; the canvas only
		 * surfaces it on hover. */
		tooltipLabels?: Record<string, string>;
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
		/** Hidden member ids behind collapsed titles; never valid endpoints. */
		hiddenIds?: string[];
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
		/** Rejected connect attempt with a human-readable reason. */
		onconnectreject?: (reason: string) => void;
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
		failedIds = [],
		criticalIds = [],
		heatTierById = {},
		decisionIds = [],
		tooltipLabels = {},
		positions = undefined,
		editMode = false,
		edgeLabelLimit = 60,
		heightClass = 'h-96',
		class: className = '',
		collapsedIds = [],
		hiddenIds = [],
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
		onconnectreject,
	}: Props = $props();

	let wrapper: HTMLDivElement | null = $state(null);
	let container: HTMLDivElement | null = $state(null);
	let cy: Core | null = $state(null);
	let ready = $state(false);
	let empty = $derived(nodes.length === 0);
	// Zoomed-out canvases hide non-essential edge labels; the flag only
	// flips when crossing the threshold so zoom gestures stay cheap.
	let zoomedOut = $state(false);
	let hover = new HoverTip();
	const hoverTip = $derived(hover.current);

	function clearHoverTip(): void {
		hover.clear();
	}

	function requestHoverTip(id: string, clientX: number, clientY: number): void {
		hover.request(
			id,
			clientX,
			clientY,
			tooltipLabels,
			wrapper?.getBoundingClientRect() ?? null,
		);
	}

	const highlight = $derived(new Set(highlightIds));
	const problems = $derived(new Set(problemIds));
	const pulses = $derived(new Set(pulseIds));
	const failed = $derived(new Set(failedIds));
	const criticals = $derived(new Set(criticalIds));
	const decisions = $derived(new Set(decisionIds));
	const collapsed = $derived(new Set(collapsedIds));
	// Latest props for gesture handlers registered once on mount. Derived
	// values stay current without snapshot effects.
	const selectedSnapshot = $derived(selectedId);
	const editSnapshot = $derived(editMode);
	const connectHandler = $derived(onconnect);
	const connectRejectHandler = $derived(onconnectreject);
	const selectHandler = $derived(onselect);
	const expandHandler = $derived(onexpand);
	// Snapshots for gesture handlers registered once on mount.
	const positionsSnapshot = $derived(positions);
	const groupTitlesSnapshot = $derived(groupTitles);
	const hiddenSnapshot = $derived(new Set(hiddenIds));
	const groupMoveHandler = $derived(ongroupmove);
	const moveHandler = $derived(onmovenode);
	const contextHandler = $derived(oncontext);
	const grabStart = new SvelteMap<string, CanvasPosition>();

	let overview = $state<MiniOverview | null>(null);
	let miniDrag = $state(false);
	let lastMiniRefresh = 0;

	const showMinimap = $derived(
		minimap === 'on' ||
			(minimap === 'auto' && nodes.length >= MINIMAP_AUTO_THRESHOLD),
	);

	/**
	 * Snapshot node dots, group outlines plus the viewport for the minimap.
	 * Structure only: execution colors stay on the main canvas so hot
	 * updates never redraw the overview.
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
		const boxes: Array<{
			id: string;
			x1: number;
			y1: number;
			x2: number;
			y2: number;
		}> = [];
		core
			.nodes()
			.filter((node) => node.isParent())
			.forEach((parent) => {
				let x1 = Number.POSITIVE_INFINITY;
				let y1 = Number.POSITIVE_INFINITY;
				let x2 = Number.NEGATIVE_INFINITY;
				let y2 = Number.NEGATIVE_INFINITY;
				let count = 0;
				parent.children().forEach((child) => {
					const position = child.position();
					x1 = Math.min(x1, position.x);
					y1 = Math.min(y1, position.y);
					x2 = Math.max(x2, position.x);
					y2 = Math.max(y2, position.y);
					count += 1;
				});
				if (count === 0) return;
				const pad = 30;
				boxes.push({
					id: parent.id(),
					x1: x1 - pad,
					y1: y1 - pad,
					x2: x2 + pad,
					y2: y2 + pad,
				});
			});
		core
			.nodes()
			.filter(
				(node) =>
					!node.isParent() && (node.id() as string).startsWith('group:'),
			)
			.forEach((title) => {
				const position = title.position();
				boxes.push({
					id: title.id(),
					x1: position.x - 80,
					y1: position.y - 20,
					x2: position.x + 80,
					y2: position.y + 20,
				});
			});
		const extent = core.extent();
		overview = computeMiniOverview(items, boxes, extent);
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
		return projectToMini(x, y, bounds);
	}

	function miniPoint(event: PointerEvent): { x: number; y: number } {
		const svg = event.currentTarget as SVGSVGElement;
		const rect = svg.getBoundingClientRect();
		return miniPointerToWorld(
			event.clientX,
			event.clientY,
			rect,
			overview?.bounds,
		);
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

	let hotspots = $state<ConnectSpot[]>([]);
	let connectDrag = $state<ConnectDrag | null>(null);

	/** Hotspot dots at node edges; edit mode only, hidden while connecting. */
	function refreshHotspots(): void {
		const core = cy;
		if (!core || !editSnapshot || connectDrag) {
			if (hotspots.length > 0) hotspots = [];
			return;
		}
		const zoom = core.zoom();
		const spots: ConnectSpot[] = [];
		core
			.nodes()
			.filter((node) => {
				if (node.isParent()) return false;
				const id = node.id() as string;
				if (isGroupTitleId(id)) return false;
				if (hiddenSnapshot.has(id)) return false;
				return true;
			})
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

	function connectRejectReason(source: string, target: string): string | null {
		return describeConnectRejection({
			editMode: editSnapshot,
			source,
			target,
			edges,
			hiddenIds: hiddenSnapshot,
		});
	}

	function connectValid(source: string, target: string): boolean {
		return isConnectValid({
			editMode: editSnapshot,
			source,
			target,
			edges,
			hiddenIds: hiddenSnapshot,
		});
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

	function setNodesGrabbable(enabled: boolean): void {
		const core = cy;
		if (!core) return;
		if (enabled && editSnapshot) {
			core.autoungrabify(false);
			core.nodes().grabify();
		} else {
			core.nodes().ungrabify();
			core.autoungrabify(true);
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
		setNodesGrabbable(true);
		refreshHotspots();
		if (!commit || !drag || !drag.target) return;
		const reason = connectRejectReason(drag.source, drag.target);
		if (reason === null) {
			connectHandler?.(drag.source, drag.target);
		} else if (reason !== 'Edge already exists.') {
			connectRejectHandler?.(reason);
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
		if (!editSnapshot) {
			connectRejectHandler?.('Read-only canvas. Enter edit mode to connect.');
			return;
		}
		if (hiddenSnapshot.has(spot.id) || isGroupTitleId(spot.id)) {
			connectRejectHandler?.('Hidden group members cannot connect.');
			return;
		}
		endConnectDrag(false);
		connectDrag = {
			source: spot.id,
			sx: spot.x,
			sy: spot.y,
			px: spot.x,
			py: spot.y,
			target: null,
		};
		setNodesGrabbable(false);
		refreshHotspots();
		window.addEventListener('pointermove', onConnectMove);
		window.addEventListener('pointerup', onConnectUp);
		window.addEventListener('keydown', onConnectKey);
	}

	function presetPositions(): Record<string, { x: number; y: number }> {
		return presetPositionsFor(nodes, edges, layout);
	}

	function elementDefs(): ElementDefinition[] {
		return buildElementDefs({
			nodes,
			edges,
			preset,
			positions,
			computed: presetPositions(),
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
			rankedEdgeIds: rankedEdgeIds(),
		});
	}

	const rankedCache = new SvelteMap<string, Set<string>>();

	function rankedEdgeIds(): Set<string> {
		const key = `${edges.length}:${edgeLabelLimit}:${selectedId ?? ''}:${[...highlight].sort().join(',')}`;
		const cached = rankedCache.get(key);
		if (cached) return cached;
		const ranked = rankEdgeIds(edges, edgeLabelLimit, selectedId, highlight);
		rankedCache.clear();
		rankedCache.set(key, ranked);
		return ranked;
	}

	function layoutOptions(): Record<string, unknown> {
		return canvasLayoutOptions(layout);
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
				style: CANVAS_STYLESHEET,
			});
			function livePositions(): Record<string, CanvasPosition> {
				const core = cy;
				const table: Record<string, CanvasPosition> = {};
				if (core) {
					core
						.nodes()
						.filter((node) => !node.isParent())
						.forEach((node) => {
							table[node.id() as string] = roundPosition(
								node.position() as CanvasPosition,
							);
						});
				}
				return { ...(positionsSnapshot ?? {}), ...table };
			}

			function withPushes(moves: CanvasMove[]): CanvasMove[] {
				if (moves.length === 0) return moves;
				const pushed = pushOverlapped(moves, livePositions());
				if (pushed.length === 0) return moves;
				const seen = new Set(moves.map((move) => move.id));
				const extra = pushed.filter((move) => !seen.has(move.id));
				return [...moves, ...extra];
			}

			instance.on('tap', 'node', (event) => {
				clearHoverTip();
				const id = event.target.id() as string;
				if (connectDrag) return;
				if (editSnapshot) {
					const original = (event as unknown as { originalEvent?: MouseEvent })
						.originalEvent;
					const from = selectedSnapshot;
					if (original?.shiftKey && from && from !== id) {
						const reason = connectRejectReason(from, id);
						if (reason === null) {
							connectHandler?.(from, id);
							return;
						}
						if (reason !== 'Edge already exists.') {
							connectRejectHandler?.(reason);
						} else {
							return;
						}
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
				clearHoverTip();
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
					groupMoveHandler?.(
						withPushes(
							titleDragMoves(
								id,
								position,
								grabStart,
								positionsSnapshot,
								groupTitlesSnapshot,
							),
						),
					);
					return;
				}
				if (id.startsWith('groupbox:')) {
					// A group box moves as one unit; report every member so
					// the store persists the drag as a single history entry.
					const moves: CanvasMove[] = [];
					target.children().forEach((child: NodeSingular) => {
						moves.push({
							id: child.id() as string,
							position: roundPosition(child.position() as CanvasPosition),
						});
					});
					if (moves.length > 0) {
						groupMoveHandler?.(withPushes(moves));
						return;
					}
				}
				// Expanded members move alone; pushed neighbors join the same
				// batched move so overlap resolution undoes atomically.
				const single: CanvasMove[] = [{ id, position }];
				const combined = withPushes(single);
				if (combined.length > 1) {
					groupMoveHandler?.(combined);
					return;
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
			instance.on('mouseover', 'node', (event) => {
				const original = (event as unknown as { originalEvent?: MouseEvent })
					.originalEvent;
				requestHoverTip(
					event.target.id() as string,
					original?.clientX ?? 0,
					original?.clientY ?? 0,
				);
			});
			instance.on('mouseout', 'node', () => {
				clearHoverTip();
			});
			const core = instance;
			core.on('zoom', () => {
				clearHoverTip();
				const out = core.zoom() < 0.6;
				if (out !== zoomedOut) zoomedOut = out;
				requestMiniRefresh();
				refreshHotspots();
			});
			core.on('pan', () => {
				clearHoverTip();
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
			clearHoverTip();
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
		void failedIds;
		void criticalIds;
		void heatTierById;
		void decisionIds;
		void positions;
		void collapsedIds;
		void hiddenIds;
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
		if (connectDrag) return;
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
	{#if hoverTip}
		<div
			class="pointer-events-none absolute z-20 max-w-56 rounded-md border border-border bg-card px-2 py-1 text-micro text-foreground shadow-md"
			style:left={`${hoverTip.x}px`}
			style:top={`${hoverTip.y}px`}
			aria-hidden="true"
		>
			{hoverTip.text}
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
				{#each overview.boxes as box (box.id)}
					{@const boxTopLeft = miniXY(box.x1, box.y1)}
					{@const boxBottomRight = miniXY(box.x2, box.y2)}
					<rect
						x={Math.min(boxTopLeft.x, boxBottomRight.x)}
						y={Math.min(boxTopLeft.y, boxBottomRight.y)}
						width={Math.max(2, Math.abs(boxBottomRight.x - boxTopLeft.x))}
						height={Math.max(2, Math.abs(boxBottomRight.y - boxTopLeft.y))}
						class="fill-transparent stroke-muted-foreground"
						stroke-width="1"
						stroke-dasharray="3 2"
					/>
				{/each}
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
