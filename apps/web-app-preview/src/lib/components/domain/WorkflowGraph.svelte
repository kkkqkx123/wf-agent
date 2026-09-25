<script lang="ts">
	import type { WorkflowGraph } from '$lib/types/models';
	import Icon from '$lib/components/icons/Icon.svelte';
	import { statusTone } from '$lib/utils/status';
	import { cn } from '$lib/utils/cn';

	interface Props {
		graph: WorkflowGraph;
		selectedId?: string | null;
		class?: string;
		onselect?: (id: string) => void;
	}

	let {
		graph,
		selectedId = null,
		class: className = '',
		onselect,
	}: Props = $props();

	const NODE_WIDTH = 132;
	const NODE_HEIGHT = 44;
	const PADDING = 24;

	const bounds = $derived({
		width:
			Math.max(...graph.nodes.map((node) => node.x + NODE_WIDTH), 0) +
			PADDING * 2,
		height:
			Math.max(...graph.nodes.map((node) => node.y + NODE_HEIGHT), 0) +
			PADDING * 2,
	});

	const nodeById = $derived(
		new Map(graph.nodes.map((node) => [node.id, node])),
	);

	function center(node: { x: number; y: number }): { x: number; y: number } {
		return { x: node.x + NODE_WIDTH / 2, y: node.y + NODE_HEIGHT / 2 };
	}

	const TONE_FILL = {
		success: 'fill-success/15 stroke-success',
		danger: 'fill-destructive/15 stroke-destructive',
		running: 'fill-running/15 stroke-running',
		warning: 'fill-warning/15 stroke-warning',
		info: 'fill-info/15 stroke-info',
		neutral: 'fill-card stroke-border',
	} as const;

	const TONE_TEXT = {
		success: 'text-success',
		danger: 'text-destructive',
		running: 'text-running',
		warning: 'text-warning',
		info: 'text-info',
		neutral: 'text-muted-foreground',
	} as const;
</script>

<div
	class={cn(
		'relative overflow-auto rounded-lg border border-border bg-muted/30',
		className,
	)}
>
	<svg
		viewBox="0 0 {bounds.width} {bounds.height}"
		width={bounds.width}
		height={bounds.height}
		role="img"
		aria-label="Workflow graph"
		class="block"
	>
		<defs>
			<marker
				id="graph-arrow"
				viewBox="0 0 8 8"
				refX="7"
				refY="4"
				markerWidth="6"
				markerHeight="6"
				orient="auto-start-reverse"
			>
				<path
					d="M0 0 L8 4 L0 8 z"
					fill="currentColor"
					class="text-muted-foreground"
				/>
			</marker>
		</defs>

		{#each graph.edges as edge (edge.id)}
			{@const from = nodeById.get(edge.from)}
			{@const to = nodeById.get(edge.to)}
			{#if from && to}
				{@const a = center(from)}
				{@const b = center(to)}
				<line
					x1={a.x}
					y1={a.y}
					x2={b.x}
					y2={b.y}
					stroke="hsl(var(--border))"
					stroke-width="1.5"
					marker-end="url(#graph-arrow)"
				/>
				{#if edge.label}
					<text
						x={(a.x + b.x) / 2}
						y={(a.y + b.y) / 2 - 6}
						text-anchor="middle"
						class="fill-[hsl(var(--muted-foreground))] text-[10px]"
					>
						{edge.label}
					</text>
				{/if}
			{/if}
		{/each}

		{#each graph.nodes as node (node.id)}
			{@const tone = statusTone(node.status)}
			<g
				role="button"
				tabindex="0"
				aria-label={node.label}
				class="cursor-pointer"
				onclick={() => onselect?.(node.id)}
				onkeydown={(event) => {
					if (event.key === 'Enter' || event.key === ' ') onselect?.(node.id);
				}}
			>
				<rect
					x={node.x}
					y={node.y}
					width={NODE_WIDTH}
					height={NODE_HEIGHT}
					rx="8"
					class={cn(
						'stroke-1.5 transition-colors',
						TONE_FILL[tone],
						selectedId === node.id && 'stroke-[hsl(var(--ring))] stroke-2',
					)}
				/>
				<text
					x={node.x + 12}
					y={node.y + 20}
					class="fill-[hsl(var(--foreground))] text-[11px] font-medium"
				>
					{node.label}
				</text>
				<text
					x={node.x + 12}
					y={node.y + 34}
					class={cn('text-[9px] uppercase tracking-wide', TONE_TEXT[tone])}
				>
					{node.kind}{node.status ? ` · ${node.status}` : ''}
				</text>
			</g>
		{/each}
	</svg>

	<div
		class="pointer-events-none absolute bottom-2 right-2 flex items-center gap-1 rounded-md border border-border bg-card/90 px-2 py-1 text-micro text-muted-foreground"
	>
		<Icon name="zoom-in" size={12} />
		<span>{graph.nodes.length} nodes · {graph.edges.length} edges</span>
	</div>
</div>
