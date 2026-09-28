<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { browser } from '$app/environment';
	import type cytoscape from 'cytoscape';
	import type { Core, ElementDefinition } from 'cytoscape';
	import {
		columnPositions,
		isDashedEdge,
		layeredPositions,
		nodeShape,
		scoreEdgeLabel,
		shortLabel,
		statusHex,
		type DisplayEdge,
		type DisplayNode,
		type GraphLayoutKind,
		type GraphPreset,
	} from '$lib/graph/display-model';
	import { cn } from '$lib/utils/cn';

	export interface CanvasPosition {
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
		/** Position overrides (edit store); unset nodes use the preset layout. */
		positions?: Record<string, CanvasPosition>;
		/** Controlled edit mode: no auto layout, gestures emit intents. */
		editMode?: boolean;
		edgeLabelLimit?: number;
		/** Height class for the canvas container (mini maps use h-56). */
		heightClass?: string;
		class?: string;
		onselect?: (id: string) => void;
		onexpand?: (id: string) => void;
		onboxselect?: (ids: string[]) => void;
		onmovenode?: (id: string, position: CanvasPosition) => void;
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
		positions = undefined,
		editMode = false,
		edgeLabelLimit = 60,
		heightClass = 'h-96',
		class: className = '',
		onselect,
		onexpand,
		onboxselect,
		onmovenode,
		onbackgrounddoubleclick,
		ondeleteedge,
		onconnect,
	}: Props = $props();

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
	// Latest selection for gesture handlers registered once on mount.
	let selectedSnapshot = $state<string | null>(selectedId);
	let editSnapshot = $state(editMode);
	$effect(() => {
		selectedSnapshot = selectedId;
	});
	$effect(() => {
		editSnapshot = editMode;
	});
	let connectHandler = $state<((source: string, target: string) => void) | undefined>(
		onconnect,
	);
	$effect(() => {
		connectHandler = onconnect;
	});
	let selectHandler = $state<((id: string) => void) | undefined>(onselect);
	$effect(() => {
		selectHandler = onselect;
	});
	let expandHandler = $state<((id: string) => void) | undefined>(onexpand);
	$effect(() => {
		expandHandler = onexpand;
	});

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
		const defs: ElementDefinition[] = nodes.map((node) => ({
			group: 'nodes' as const,
			data: {
				id: node.id,
				label: shortLabel(node.label),
				fullLabel: node.label,
				kind: node.kind,
				shape: nodeShape(node.kind, preset),
				color: statusHex(node.status),
			},
			position: positions?.[node.id] ?? computed[node.id],
			classes: [
				selectedId === node.id ? 'selected' : '',
				problems.has(node.id) ? 'problem' : '',
				pulses.has(node.id) ? 'running' : '',
				criticals.has(node.id) ? 'critical' : '',
				highlight.size > 0
					? highlight.has(node.id)
						? 'highlighted'
						: 'dimmed'
					: '',
			]
				.filter(Boolean)
				.join(' '),
		}));
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
					color: statusHex(edge.status) === '#71717a' ? '#71717a' : statusHex(edge.status),
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
				const classes = (def.classes ?? '') as string;
				element.classes(classes);
				if (def.position && element.isNode()) {
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

	export function relayout(): void {
		runLayout();
	}

	/** Ids currently selected on the canvas (edit toolbar delete). */
	export function selectedNodeIds(): string[] {
		const boxed =
			cy?.$('node:selected').map((node) => node.id() as string) ?? [];
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
						selector: 'edge.dimmed',
						style: { opacity: 0.25 },
					},
				] as unknown as cytoscape.StylesheetJson,
			});
			instance.on('tap', 'node', (event) => {
				const id = event.target.id() as string;
				if (editSnapshot) {
					const original = (
						event as unknown as { originalEvent?: MouseEvent }
					).originalEvent;
					const from = selectedSnapshot;
					if (
						original?.shiftKey &&
						from &&
						from !== id &&
						connectHandler
					) {
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
			instance.on('dragfree', 'node', (event) => {
				if (!editMode) return;
				const target = event.target;
				const id = target.id() as string;
				const position = target.position() as CanvasPosition;
				onmovenode?.(id, {
					x: Math.round(position.x),
					y: Math.round(position.y),
				});
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
		void positions;
		void preset;
		void edgeLabelLimit;
		void zoomedOut;
		syncElements();
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
	});

	$effect(() => {
		if (!ready || editMode) return;
		void layout;
		runLayout();
	});
</script>

<div
	class={cn(
		'relative overflow-hidden rounded-lg border border-border bg-muted/30',
		className,
	)}
>
	<div
		bind:this={container}
		class={`w-full ${heightClass}`}
		role="img"
		aria-label="Graph canvas"
	></div>
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
</div>
