<script lang="ts">
	import type { Execution } from '#lib/types/models';
	import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
	import { legendFor } from '#lib/graph/display-model';
	import GraphCanvas from '#lib/components/domain/GraphCanvas.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import Progress from '#lib/ui/components/Progress.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import { preferences } from '#lib/stores/preferences.svelte';
	import { statusTone } from '#lib/ui/status';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';

	interface Props {
		execution: Execution;
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		loading?: boolean;
		error?: string | null;
		onretry?: () => void;
		pulseIds?: string[];
		failedIds?: string[];
		criticalIds?: string[];
		heatTierById?: Record<string, number>;
		decisionIds?: string[];
		heatLabels?: Record<string, string>;
		decisionLabels?: Record<string, string>;
		selectedId?: string | null;
		onselect?: (id: string | null) => void;
		executedCount?: number;
		slowCount?: number;
		decisionCount?: number;
		liveCount?: number;
		isLive?: boolean;
		streamState?: string;
		onGoGraph?: () => void;
		onFocusNode?: (id: string) => void;
	}

	let {
		execution,
		nodes,
		edges,
		loading = false,
		error = null,
		onretry,
		pulseIds = [],
		failedIds = [],
		criticalIds = [],
		heatTierById = {},
		decisionIds = [],
		heatLabels = {},
		decisionLabels = {},
		selectedId = null,
		onselect,
		executedCount = 0,
		slowCount = 0,
		decisionCount = 0,
		liveCount = 0,
		isLive = false,
		streamState = 'closed',
		onGoGraph,
		onFocusNode,
	}: Props = $props();

	let canvas: GraphCanvas | null = $state(null);
	let legendOpen = $derived(preferences.executionFloatLegend);
	let expanded = $derived(preferences.executionFloatExpanded);
	let hidden = $derived(preferences.executionFloatHidden);
	let compact = $derived(preferences.executionFloatSize === 'compact');

	const tone = $derived(statusTone(execution.status));
	const progressTone = $derived(
		tone === 'danger'
			? 'danger'
			: tone === 'success'
				? 'success'
				: tone === 'running'
					? 'running'
					: tone === 'warning'
						? 'warning'
						: 'default',
	);

	const legendEntries = $derived(legendFor('execution'));
	const failedCount = $derived(failedIds.length);
	const criticalCount = $derived(criticalIds.length);
	const runningCount = $derived(pulseIds.length);

	const definitionLabel = $derived.by(() => {
		const id = execution.workflowId?.trim() ?? '';
		if (!id) return null;
		const short = id.replace(/^workflow:/, '');
		return execution.version ? `${short} · v${execution.version}` : short;
	});

	const tooltipLabels = $derived.by(() => {
		const merged: Record<string, string> = { ...decisionLabels };
		for (const [id, text] of Object.entries(heatLabels)) {
			merged[id] = merged[id] ? `${merged[id]} · ${text}` : text;
		}
		return merged;
	});

	function toggleExpand(): void {
		preferences.setExecutionFloatExpanded(!preferences.executionFloatExpanded);
	}

	function hide(): void {
		preferences.setExecutionFloatHidden(true);
	}

	function show(): void {
		preferences.setExecutionFloatHidden(false);
	}

	function toggleLegend(): void {
		preferences.setExecutionFloatLegend(!preferences.executionFloatLegend);
	}

	function toggleSize(): void {
		preferences.setExecutionFloatSize(compact ? 'standard' : 'compact');
	}

	function handleSelect(id: string): void {
		onselect?.(id);
	}

	function handleExpand(id: string): void {
		onFocusNode?.(id);
	}
</script>

{#if hidden}
	<div class="pointer-events-none absolute right-4 bottom-4 z-30 flex justify-end">
		<div class="pointer-events-auto">
			<Button variant="outline" size="sm" onclick={show}>
				{t('domain.float.showGraph')}
			</Button>
		</div>
	</div>
{:else if !expanded}
	<div
		class="pointer-events-none absolute right-4 bottom-4 z-30 flex w-72 max-w-[calc(100%-2rem)] justify-end"
		role="complementary"
		aria-label={t('domain.float.collapsedLabel')}
	>
		<div
			class="surface-float pointer-events-auto w-full rounded-lg border border-border px-2.5 py-2 shadow-popover"
		>
			<div class="flex items-center justify-between gap-2">
				<StatusBadge status={execution.status} size="sm" />
				<span class="text-micro tabular-nums text-muted-foreground">
					{t('domain.executionCard.tasks', {
						done: execution.tasksDone,
						total: execution.tasksTotal,
					})}
				</span>
				<span class="flex items-center gap-1">
					<IconButton
						icon="maximize"
						label={t('domain.float.expand')}
						iconSize={13}
						compact
						onclick={toggleExpand}
					/>
					<IconButton
						icon="x"
						label={t('domain.float.hide')}
						iconSize={13}
						compact
						onclick={hide}
					/>
				</span>
			</div>
			<div class="mt-1.5">
				<Progress value={execution.progress} tone={progressTone} />
			</div>
			<div class="mt-1.5 flex items-center justify-between gap-2">
				<p class="min-w-0 truncate font-mono text-micro text-muted-foreground" title={execution.currentNode ?? ''}>
					{#if isLive}
						<span class="mr-1 inline-block h-1.5 w-1.5 animate-pulse-dot rounded-full bg-running"></span>
					{/if}
					{execution.currentNode ?? t('domain.inspector.noActiveNode')}
				</p>
				{#if failedCount > 0}
					<span class="shrink-0 text-micro font-medium text-destructive">
						{t('domain.float.failedCount', { count: failedCount })}
					</span>
				{/if}
			</div>
			{#if definitionLabel}
				<p class="mt-1 truncate font-mono text-micro text-muted-foreground" title={definitionLabel}>
					{definitionLabel}
				</p>
			{/if}
		</div>
	</div>
{:else}
	<section
		class={cn(
			'pointer-events-none absolute right-4 bottom-4 z-30 flex max-w-[calc(100%-2rem)] justify-end',
			compact ? 'w-80' : 'w-96',
		)}
		role="complementary"
		aria-label={t('domain.float.expandedLabel')}
	>
		<div
			class="surface-float pointer-events-auto flex w-full min-h-0 flex-col rounded-lg border border-border shadow-popover"
		>
			<div class="flex items-center justify-between gap-2 border-b border-border px-2.5 py-2">
				<div class="flex min-w-0 items-center gap-2">
					<StatusBadge status={execution.status} size="sm" />
					<span class="text-micro tabular-nums text-muted-foreground">
						{t('domain.executionCard.tasks', {
							done: execution.tasksDone,
							total: execution.tasksTotal,
						})}
					</span>
					{#if isLive}
						<span class="flex shrink-0 items-center gap-1 text-micro text-muted-foreground">
							<span class="h-1.5 w-1.5 animate-pulse-dot rounded-full bg-running"></span>
							{liveCount > 0
								? t('domain.inspector.liveNodes', { count: liveCount })
								: streamState}
						</span>
					{/if}
				</div>
				<span class="flex shrink-0 items-center gap-0.5">
					<IconButton
						icon="minimize"
						label={t('domain.float.collapse')}
						iconSize={13}
						compact
						onclick={toggleExpand}
					/>
					<IconButton
						icon="workflow"
						label={t('domain.float.viewInGraph')}
						iconSize={13}
						compact
						onclick={() => onGoGraph?.()}
					/>
					<IconButton
						icon="x"
						label={t('domain.float.hide')}
						iconSize={13}
						compact
						onclick={hide}
					/>
				</span>
			</div>
			<div class="px-2.5 pt-2">
				<Progress value={execution.progress} tone={progressTone} />
				<div class="mt-1 flex items-center justify-between gap-2">
					<p
						class="min-w-0 truncate font-mono text-micro text-muted-foreground"
						title={execution.currentNode ?? ''}
					>
						{execution.currentNode ?? t('domain.inspector.noActiveNode')}
					</p>
					<button
						type="button"
						class="shrink-0 text-micro text-muted-foreground underline-offset-2 hover:underline"
						onclick={toggleSize}
					>
						{compact ? t('domain.float.sizeStandard') : t('domain.float.sizeCompact')}
					</button>
				</div>
				{#if definitionLabel}
					<p class="mt-0.5 truncate font-mono text-micro text-muted-foreground" title={definitionLabel}>
						{t('domain.float.templateLabel', { label: definitionLabel })}
					</p>
				{/if}
			</div>
			<div class="min-h-0 px-2.5 py-2">
				{#if loading}
					<div
						class={cn(
							'flex items-center justify-center rounded-md border border-border bg-muted/30 text-caption text-muted-foreground',
							compact ? 'h-40' : 'h-56',
						)}
						role="status"
					>
						{t('domain.canvas.loadingRenderer')}
					</div>
				{:else if error}
					<div
						class={cn(
							'flex flex-col items-center justify-center gap-2 rounded-md border border-border bg-card px-2 py-4 text-center',
							compact ? 'h-40' : 'h-56',
						)}
					>
						<p class="text-caption text-destructive">{error}</p>
						{#if onretry}
							<Button variant="outline" size="sm" onclick={() => onretry?.()}>
								{t('common.retry')}
							</Button>
						{/if}
					</div>
				{:else if nodes.length === 0}
					<div
						class={cn(
							'flex items-center justify-center rounded-md border border-border bg-muted/30 text-caption text-muted-foreground',
							compact ? 'h-40' : 'h-56',
						)}
					>
						{t('domain.canvas.noNodes')}
					</div>
				{:else}
					<GraphCanvas
						bind:this={canvas}
						nodes={nodes}
						edges={edges}
						preset="execution"
						selectedId={selectedId}
						pulseIds={pulseIds}
						failedIds={failedIds}
						criticalIds={criticalIds}
						heatTierById={heatTierById}
						decisionIds={decisionIds}
						tooltipLabels={tooltipLabels}
						minimap="off"
						heightClass={compact ? 'h-40' : 'h-56'}
						onselect={handleSelect}
						onexpand={handleExpand}
					/>
				{/if}
			</div>
			<div class="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-border px-2.5 py-2 text-micro text-muted-foreground">
				<span class="tabular-nums" title={t('domain.float.executedHint')}>
					{t('domain.float.executedCount', { count: executedCount })}
				</span>
				{#if failedCount > 0}
					<span class="tabular-nums font-medium text-destructive">
						{t('domain.float.failedCount', { count: failedCount })}
					</span>
				{/if}
				{#if criticalCount > 0}
					<span class="tabular-nums">
						{t('domain.float.criticalCount', { count: criticalCount })}
					</span>
				{/if}
				{#if slowCount > 0}
					<span class="tabular-nums">
						{t('domain.float.slowCount', { count: slowCount })}
					</span>
				{/if}
				{#if decisionCount > 0}
					<span class="tabular-nums">
						{t('domain.float.decisionCount', { count: decisionCount })}
					</span>
				{/if}
				{#if runningCount > 0}
					<span class="tabular-nums text-running">
						{t('domain.float.runningCount', { count: runningCount })}
					</span>
				{/if}
				<span class="ml-auto flex items-center gap-1">
					<button
						type="button"
						class="underline-offset-2 hover:underline"
						onclick={toggleLegend}
						aria-expanded={legendOpen}
					>
						{t('domain.graph.legend')}
					</button>
					<button
						type="button"
						class="underline-offset-2 hover:underline"
						onclick={() => onGoGraph?.()}
					>
						{t('domain.float.viewInGraph')}
					</button>
				</span>
			</div>
			{#if legendOpen}
				<div class="max-h-36 overflow-y-auto border-t border-border px-2.5 py-2">
					<ul class="grid grid-cols-2 gap-x-3 gap-y-1">
						{#each legendEntries as entry (entry.label)}
							<li class="flex items-center gap-1.5 text-micro text-muted-foreground">
								<span
									class="inline-block h-2.5 w-2.5 shrink-0"
									style:background={entry.shape.startsWith('line') ? 'transparent' : entry.color}
									style:border-radius={entry.shape === 'ellipse'
										? '9999px'
										: entry.shape === 'diamond'
											? '2px'
											: '4px'}
									style:transform={entry.shape === 'diamond' ? 'rotate(45deg)' : 'none'}
									style:border={entry.shape.startsWith('line')
										? `2px ${entry.shape === 'line-dashed' ? 'dashed' : 'solid'} ${entry.color}`
										: 'none'}
									aria-hidden="true"
								></span>
								<span class="truncate">{entry.label}</span>
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		</div>
	</section>
{/if}
