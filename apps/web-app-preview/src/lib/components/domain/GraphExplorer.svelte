<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import GraphCanvas from '$lib/components/domain/GraphCanvas.svelte';
	import {
		capGraph,
		distinctKinds,
		legendFor,
		type DisplayEdge,
		type DisplayNode,
		type GraphLayoutKind,
		type GraphPreset,
	} from '$lib/graph/display-model';
	import { toasts } from '$lib/stores/toast.svelte';
	import { cn } from '$lib/utils/cn';

	export interface GraphOverlay {
		id: string;
		label: string;
		ids: string[];
	}

	interface Props {
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		preset: GraphPreset;
		loading?: boolean;
		error?: string | null;
		onretry?: () => void;
		selectedId?: string | null;
		onselect?: (id: string | null) => void;
		onexpand?: (id: string) => void;
		expandLabel?: string;
		overlays?: GraphOverlay[];
		activeOverlay?: string | null;
		onoverlay?: (id: string | null) => void;
		inspector?: Snippet;
		actions?: Snippet;
		class?: string;
	}

	let {
		nodes,
		edges,
		preset,
		loading = false,
		error = null,
		onretry,
		selectedId = null,
		onselect,
		onexpand,
		expandLabel = 'Load neighbors',
		overlays = [],
		activeOverlay = null,
		onoverlay,
		inspector,
		actions,
		class: className = '',
	}: Props = $props();

	let canvas: GraphCanvas | null = $state(null);
	function initialLayout(kind: GraphPreset): GraphLayoutKind {
		return kind === 'decision' ? 'columns' : 'layered';
	}
	let layout = $state<GraphLayoutKind>(initialLayout(preset));
	let showFilters = $state(false);
	let query = $state('');
	let hiddenKinds = $state<string[]>([]);

	const kinds = $derived(distinctKinds(nodes));
	const activeIds = $derived(
		new Set(overlays.find((overlay) => overlay.id === activeOverlay)?.ids ?? []),
	);

	const filtered = $derived.by(() => {
		const hidden = new Set(hiddenKinds);
		const needle = query.trim().toLowerCase();
		const keptNodes = nodes.filter((node) => {
			if (hidden.has(node.kind || 'unknown')) return false;
			if (
				needle &&
				!node.label.toLowerCase().includes(needle) &&
				!node.id.toLowerCase().includes(needle)
			) {
				return false;
			}
			return true;
		});
		const kept = new Set(keptNodes.map((node) => node.id));
		return {
			nodes: keptNodes,
			edges: edges.filter(
				(edge) => kept.has(edge.source) && kept.has(edge.target),
			),
		};
	});

	const capped = $derived(capGraph(filtered.nodes, filtered.edges));

	const selected = $derived(
		nodes.find((node) => node.id === selectedId) ?? null,
	);

	const selectedNeighbors = $derived.by(() => {
		if (!selectedId) return { predecessors: 0, successors: 0 };
		let predecessors = 0;
		let successors = 0;
		for (const edge of edges) {
			if (edge.target === selectedId) predecessors += 1;
			if (edge.source === selectedId) successors += 1;
		}
		return { predecessors, successors };
	});

	function toggleKind(kind: string): void {
		hiddenKinds = hiddenKinds.includes(kind)
			? hiddenKinds.filter((entry) => entry !== kind)
			: [...hiddenKinds, kind];
	}

	function toggleOverlay(id: string): void {
		onoverlay?.(activeOverlay === id ? null : id);
	}

	function handleExport(): void {
		const done = canvas?.exportPng() ?? false;
		if (done) {
			toasts.success('Graph exported as PNG');
		} else {
			toasts.error('Graph export failed', 'The renderer is not ready yet.');
		}
	}

	const layoutOptions = [
		{ value: 'layered', label: 'Layered' },
		{ value: 'columns', label: 'Columns' },
		{ value: 'force', label: 'Force' },
		{ value: 'grid', label: 'Grid' },
	];
</script>

<div class={cn('flex min-h-0 flex-col gap-2', className)}>
	<div class="flex flex-wrap items-center gap-1.5">
		<IconButton
			icon="plus"
			label="Zoom in"
			onclick={() => canvas?.zoomIn()}
		/>
		<IconButton
			icon="minus"
			label="Zoom out"
			onclick={() => canvas?.zoomOut()}
		/>
		<IconButton icon="maximize" label="Fit to view" onclick={() => canvas?.fit()} />
		<IconButton
			icon="refresh"
			label="Re-run layout"
			onclick={() => canvas?.relayout()}
		/>
		<Select
			bind:value={layout}
			options={layoutOptions}
			size="sm"
			placeholder=""
			class="w-28"
		/>
		<IconButton
			icon="download"
			label="Export as PNG"
			onclick={handleExport}
		/>
		<IconButton
			icon="filter"
			label={showFilters ? 'Hide filters' : 'Show filters'}
			onclick={() => (showFilters = !showFilters)}
		/>
		{#if actions}
			<span class="mx-1 h-5 w-px bg-border"></span>
			{@render actions()}
		{/if}
		<span class="ml-auto text-micro text-muted-foreground">
			{#if capped.truncated}
				Showing {capped.nodes.length} of {capped.total} nodes ·
			{/if}
			{filtered.nodes.length} nodes · {filtered.edges.length} edges
		</span>
	</div>

	{#if overlays.length > 0}
		<div class="flex flex-wrap items-center gap-1.5">
			<span class="text-micro text-muted-foreground">Highlight:</span>
			{#each overlays as overlay (overlay.id)}
				<Button
					variant={activeOverlay === overlay.id ? 'default' : 'outline'}
					size="sm"
					onclick={() => toggleOverlay(overlay.id)}
				>
					{overlay.label}
					<Badge variant="neutral" class="ml-1">{overlay.ids.length}</Badge>
				</Button>
			{/each}
		</div>
	{/if}

	{#if showFilters}
		<Card title="Filters" class="shrink-0">
			<div class="flex flex-wrap items-center gap-2">
				<Input
					bind:value={query}
					placeholder="Search nodes…"
					class="h-7 w-44"
				/>
				{#each kinds as kind (kind)}
					<label
						class="flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-2 py-1 text-caption"
					>
						<input
							type="checkbox"
							checked={!hiddenKinds.includes(kind)}
							onchange={() => toggleKind(kind)}
							class="accent-current"
						/>
						<span class="font-mono">{kind}</span>
					</label>
				{/each}
				{#if hiddenKinds.length > 0 || query}
					<Button
						variant="ghost"
						size="sm"
						onclick={() => {
							hiddenKinds = [];
							query = '';
						}}
					>
						Clear
					</Button>
				{/if}
			</div>
		</Card>
	{/if}

	{#if loading}
		<Skeleton class="h-96 w-full rounded-lg" />
	{:else if error}
		<ErrorState
			title="Graph failed to load"
			description={error}
			{onretry}
			class="rounded-lg border border-border bg-card"
		/>
	{:else if nodes.length === 0}
		<EmptyState
			icon="workflow"
			title="No graph data"
			description="This item has no nodes yet."
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<div class="grid min-h-0 gap-2 xl:grid-cols-[1fr_16rem]">
			<GraphCanvas
				bind:this={canvas}
				nodes={capped.nodes}
				edges={capped.edges}
				{preset}
				{layout}
				{selectedId}
				highlightIds={[...activeIds]}
				onselect={(id) => onselect?.(id)}
				onexpand={(id) => onexpand?.(id)}
				class="min-h-0"
			/>
			<div class="flex min-h-0 flex-col gap-2">
				{#if capped.truncated}
					<Card title="Large graph">
						<p class="text-caption text-muted-foreground">
							Showing the first {capped.nodes.length} of {capped.total}
							nodes. Use filters or double-click a node to expand its
							neighborhood.
						</p>
					</Card>
				{/if}
				{#if selected}
					<Card title={selected.label}>
						{#snippet actions()}
							<IconButton
								icon="x"
								label="Clear selection"
								onclick={() => onselect?.(null)}
							/>
						{/snippet}
						<dl class="space-y-1 text-caption">
							<div class="flex justify-between gap-2">
								<dt class="text-muted-foreground">Kind</dt>
								<dd class="font-mono">{selected.kind}</dd>
							</div>
							{#if selected.status}
								<div class="flex justify-between gap-2">
									<dt class="text-muted-foreground">Status</dt>
									<dd><StatusBadge status={selected.status} size="sm" dot={false} /></dd>
								</div>
							{/if}
							{#if selected.iteration !== undefined}
								<div class="flex justify-between gap-2">
									<dt class="text-muted-foreground">Iteration</dt>
									<dd class="font-mono">{selected.iteration}</dd>
								</div>
							{/if}
							<div class="flex justify-between gap-2">
								<dt class="text-muted-foreground">Links</dt>
								<dd class="font-mono">
									{selectedNeighbors.predecessors} in · {selectedNeighbors.successors} out
								</dd>
							</div>
						</dl>
						{#if inspector}
							<div class="mt-2 border-t border-border pt-2">
								{@render inspector()}
							</div>
						{/if}
						{#if onexpand}
							<div class="mt-2">
								<Button
									variant="outline"
									size="sm"
									onclick={() => selected && onexpand?.(selected.id)}
								>
									<Icon name="git-commit" size={13} />
									{expandLabel}
								</Button>
							</div>
						{/if}
					</Card>
				{/if}
				<Card title="Legend">
					<ul class="space-y-1">
						{#each legendFor(preset) as entry (entry.label)}
							<li class="flex items-center gap-2 text-caption text-muted-foreground">
								<span
									class="inline-block h-2.5 w-2.5"
									style:background={entry.shape.startsWith('line')
										? 'transparent'
										: entry.color}
									style:border-radius={
										entry.shape === 'ellipse'
											? '9999px'
											: entry.shape === 'diamond'
												? '2px'
												: '4px'
									}
									style:transform={
										entry.shape === 'diamond' ? 'rotate(45deg)' : 'none'
									}
									style:border={entry.shape.startsWith('line')
										? `2px ${entry.shape === 'line-dashed' ? 'dashed' : 'solid'} ${entry.color}`
										: 'none'}
								></span>
								{entry.label}
							</li>
						{/each}
					</ul>
				</Card>
			</div>
		</div>
	{/if}
</div>
