<script lang="ts">
	import type { Snippet } from 'svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import { legendFor, type DisplayNode, type GraphPreset } from '#lib/graph/display-model';
	import { t } from '#lib/i18n';

	/** Display data for a collapsed group title selection. */
	export interface ExplorerGroupInfo {
		label: string;
		memberCount: number;
		collapsed: boolean;
	}

	/** Predecessor/successor counts for the selected node. */
	export interface ExplorerNeighbors {
		predecessors: number;
		successors: number;
	}

	/** Data for the large-graph notice card; null hides the card. */
	export interface ExplorerLargeGraphInfo {
		auto: string[];
		truncated: boolean;
		shown: number;
		total: number;
		counts: Array<{ kind: string; count: number }>;
		hiddenKindsCount: number;
	}

	interface Props {
		largeGraph: ExplorerLargeGraphInfo | null;
		selected: DisplayNode | null;
		selectedIsGroup: boolean;
		group: ExplorerGroupInfo | null;
		neighbors: ExplorerNeighbors;
		heatLabel?: string | null;
		decisionLabel?: string | null;
		inspector?: Snippet;
		canExpand: boolean;
		expandLabel: string;
		canJumpParams: boolean;
		preset: GraphPreset;
		onclearselection?: () => void;
		ontogglegroup?: () => void;
		onfocusgroup?: () => void;
		onexpand?: () => void;
		oncopyid?: () => void;
		onjumpparam?: () => void;
		onisolatekind?: (kind: string) => void;
		onshowallkinds?: () => void;
	}

	let {
		largeGraph,
		selected,
		selectedIsGroup,
		group,
		neighbors,
		heatLabel,
		decisionLabel,
		inspector,
		canExpand,
		expandLabel,
		canJumpParams,
		preset,
		onclearselection,
		ontogglegroup,
		onfocusgroup,
		onexpand,
		oncopyid,
		onjumpparam,
		onisolatekind,
		onshowallkinds,
	}: Props = $props();
</script>

<div class="flex min-h-0 flex-col gap-2">
	{#if largeGraph}
		<Card title={t('domain.graph.largeGraph')}>
			<p class="text-caption text-muted-foreground">
				{t('domain.graph.retentionOrderDetail')}
				{#if largeGraph.auto.length > 0}
					{t('domain.graph.foldedNonCritical', {
						count: largeGraph.auto.length,
						kinds: largeGraph.auto.join(', '),
					})}
				{/if}
				{#if largeGraph.truncated}
					{t('domain.graph.sampledAcrossKinds', {
						shown: largeGraph.shown,
						total: largeGraph.total,
						counts: largeGraph.counts.map((entry) => `${entry.kind} ${entry.count}`).join(' · '),
					})}
				{/if}
			</p>
			<div class="mt-2 flex flex-wrap gap-1.5">
				{#each largeGraph.counts.slice(0, 4) as entry (entry.kind)}
					<Button variant="outline" size="sm" onclick={() => onisolatekind?.(entry.kind)}>
						{t('domain.graph.foldTo', { kind: entry.kind })}
					</Button>
				{/each}
				{#if largeGraph.hiddenKindsCount > 0}
					<Button variant="ghost" size="sm" onclick={() => onshowallkinds?.()}>
						{t('domain.graph.showAllKinds')}
					</Button>
				{/if}
			</div>
		</Card>
	{/if}
	{#if selected}
		<Card title={selected.label}>
			{#snippet actions()}
				<IconButton
					icon="x"
					label={t('domain.graph.clearSelection')}
					onclick={() => onclearselection?.()}
				/>
			{/snippet}
			{#if selectedIsGroup}
				<p class="text-caption text-muted-foreground">
					{t('domain.graph.groupSummary', {
						label: group?.label ?? selected.id,
						members: group?.memberCount ?? 0,
						in: neighbors.predecessors,
						out: neighbors.successors,
					})}
				</p>
				{#if group}
					<div class="mt-2 flex flex-wrap gap-1.5">
						<Button variant="outline" size="sm" onclick={() => ontogglegroup?.()}>
							{group.collapsed ? t('domain.graph.expandGroup') : t('domain.graph.collapseGroup')}
						</Button>
						<Button variant="outline" size="sm" onclick={() => onfocusgroup?.()}>
							{t('domain.graph.focusWholeGroup')}
						</Button>
					</div>
					<p
						class="mt-1 text-micro text-muted-foreground"
						title={t('domain.graph.groupsNoDefinition')}
					>
						{t('domain.graph.locateUnavailableForGroups')}
					</p>
				{/if}
			{:else}
				<dl class="space-y-1 text-caption">
					<div class="flex justify-between gap-2">
						<dt class="text-muted-foreground">
							{t('domain.graph.kind')}
						</dt>
						<dd class="font-mono">{selected.kind}</dd>
					</div>
					{#if selected.status}
						<div class="flex justify-between gap-2">
							<dt class="text-muted-foreground">
								{t('domain.graph.status')}
							</dt>
							<dd>
								<StatusBadge status={selected.status} size="sm" dot={false} />
							</dd>
						</div>
					{/if}
					{#if selected.iteration !== undefined}
						<div class="flex justify-between gap-2">
							<dt class="text-muted-foreground">
								{t('domain.history.iterations')}
							</dt>
							<dd class="font-mono">{selected.iteration}</dd>
						</div>
					{/if}
					{#if heatLabel}
						<div class="flex justify-between gap-2">
							<dt class="text-muted-foreground">
								{t('domain.inspector.duration')}
							</dt>
							<dd class="font-mono tabular-nums">
								{heatLabel}
							</dd>
						</div>
					{/if}
					{#if decisionLabel}
						<div class="flex justify-between gap-2">
							<dt class="text-muted-foreground">
								{t('domain.graph.branches')}
							</dt>
							<dd class="text-right font-mono">
								{decisionLabel}
							</dd>
						</div>
					{/if}
					<div class="flex justify-between gap-2">
						<dt class="text-muted-foreground">
							{t('domain.graph.links')}
						</dt>
						<dd class="font-mono">
							{t('domain.graph.inOut', {
								in: neighbors.predecessors,
								out: neighbors.successors,
							})}
						</dd>
					</div>
				</dl>
				{#if inspector}
					<div class="mt-2 border-t border-border pt-2">
						{@render inspector()}
					</div>
				{/if}
				{#if canExpand}
					<div class="mt-2">
						<Button variant="outline" size="sm" onclick={() => onexpand?.()}>
							<Icon name="git-commit" size={13} />
							{expandLabel}
						</Button>
					</div>
				{/if}
				<div class="mt-2 flex flex-wrap gap-1.5">
					<Button variant="outline" size="sm" onclick={() => oncopyid?.()}>
						{t('domain.graph.copyNodeId')}
					</Button>
					{#if canJumpParams}
						<Button variant="outline" size="sm" onclick={() => onjumpparam?.()}>
							{t('domain.graph.locateInDefinition')}
						</Button>
					{:else}
						<Button
							variant="outline"
							size="sm"
							disabled
							title={t('domain.graph.noDefinitionPanel')}
						>
							{t('domain.graph.locateInDefinition')}
						</Button>
					{/if}
				</div>
				{#if !canJumpParams}
					<p class="mt-1 text-micro text-muted-foreground">
						{t('domain.graph.noDefinitionPanel')}
					</p>
				{/if}
			{/if}
		</Card>
	{/if}
	<Card title={t('domain.graph.legend')}>
		<ul class="space-y-1">
			{#each legendFor(preset) as entry (entry.label)}
				<li class="flex items-center gap-2 text-caption text-muted-foreground">
					<span
						class="inline-block h-2.5 w-2.5"
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
					></span>
					{entry.label}
				</li>
			{/each}
		</ul>
	</Card>
</div>
