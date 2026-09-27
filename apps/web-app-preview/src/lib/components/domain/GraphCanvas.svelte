<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { browser } from '$app/environment';
	import type cytoscape from 'cytoscape';
	import type { Core, ElementDefinition } from 'cytoscape';
	import {
		columnPositions,
		isDashedEdge,
		nodeShape,
		shortLabel,
		statusHex,
		type DisplayEdge,
		type DisplayNode,
		type GraphLayoutKind,
		type GraphPreset,
	} from '$lib/graph/display-model';
	import { cn } from '$lib/utils/cn';

	interface Props {
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		preset: GraphPreset;
		layout?: GraphLayoutKind;
		selectedId?: string | null;
		highlightIds?: string[];
		class?: string;
		onselect?: (id: string) => void;
		onexpand?: (id: string) => void;
	}

	let {
		nodes,
		edges,
		preset,
		layout = 'layered',
		selectedId = null,
		highlightIds = [],
		class: className = '',
		onselect,
		onexpand,
	}: Props = $props();

	let container: HTMLDivElement | null = $state(null);
	let cy: Core | null = $state(null);
	let ready = $state(false);
	let empty = $derived(nodes.length === 0);

	const highlight = $derived(new Set(highlightIds));

	function elementDefs(): ElementDefinition[] {
		const positions: Record<string, { x: number; y: number }> =
			layout === 'columns' ? Object.fromEntries(columnPositions(nodes)) : {};
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
			position: positions[node.id],
			classes: [
				selectedId === node.id ? 'selected' : '',
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
			defs.push({
				group: 'edges' as const,
				data: {
					id: edge.id,
					source: edge.source,
					target: edge.target,
					label: edge.label ?? '',
					lineStyle: isDashedEdge(edge.kind) ? 'dashed' : 'solid',
				},
			});
		}
		return defs;
	}

	function layoutOptions(): Record<string, unknown> {
		switch (layout) {
			case 'columns':
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
				return {
					name: 'breadthfirst',
					directed: true,
					padding: 30,
					spacingFactor: 1.15,
					circle: false,
					grid: true,
					fit: true,
				};
		}
	}

	function syncElements(): void {
		if (!cy) return;
		const defs = elementDefs();
		const wanted = new SvelteMap(defs.map((def) => [def.data.id as string, def]));
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

	export function relayout(): void {
		runLayout();
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
							'line-color': '#71717a',
							'line-style': 'data(lineStyle)',
							'target-arrow-shape': 'triangle',
							'target-arrow-color': '#71717a',
							'curve-style': 'bezier',
							label: 'data(label)',
							color: '#a1a1aa',
							'font-size': 9,
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
				const now = Date.now();
				if (lastTap && lastTap.id === id && now - lastTap.at < 350) {
					lastTap = null;
					onexpand?.(id);
					return;
				}
				lastTap = { id, at: now };
				onselect?.(id);
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
		void preset;
		syncElements();
	});

	$effect(() => {
		if (!ready) return;
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
	<div bind:this={container} class="h-96 w-full" role="img" aria-label="Graph canvas"></div>
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
