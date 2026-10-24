<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import type { WorkflowDetail } from '#lib/types/models';
	import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
	import type { WorkflowGraphState } from '#lib/workflows/graph-state.svelte';
	import RunsPanel from '#lib/components/domain/RunsPanel.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		detail: WorkflowDetail;
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		state: WorkflowGraphState;
		/** Enter the edit tab with the current selection. */
		onedit?: () => void;
	}

	// Selection, overlays and analysis live in the route-owned state object so
	// leaving the tab and returning keeps them; the route reloads it with the
	// detail (load/promotion), matching the pre-split page behavior.
	let { detail, nodes, edges, state: graph, onedit }: Props = $props();

	// The log strip shares the tab so graph context and recent runs stay
	// visible together; collapsed by default to preserve canvas height.
	let logsOpen = $state(false);

	const selectedNode = $derived(
		nodes.find((node) => node.id === graph.selection) ?? null,
	);
</script>

{#if graph.analysisError}
	<p class="mb-2 text-caption text-warning">
		{t('domain.workflowGraphTab.analysisUnavailable', {
			error: graph.analysisError,
		})}
		<button
			type="button"
			class="underline"
			onclick={() => void graph.loadAnalysis(detail.id)}
		>
			{t('domain.hierarchyTree.retry')}
		</button>.
	</p>
{/if}
{#await import('#lib/components/domain/GraphExplorer.svelte') then module}
<module.default
	{nodes}
	{edges}
	preset="workflow"
	layoutId={detail.id}
	selectedId={graph.selection}
	onselect={(id) => (graph.selection = id)}
	onexpand={(id) => void graph.expandNeighborhood(detail.id, id)}
	expandLabel={t('domain.inspector.revealNeighborhood')}
	overlays={graph.effectiveOverlays}
	activeOverlay={graph.activeOverlay}
	onoverlay={(id) => (graph.activeOverlay = id)}
	onjumpparam={(id) => graph.jumpToNodeRow(id)}
	ontogglelogs={() => (logsOpen = !logsOpen)}
>
	{#snippet inspector()}
		{#if selectedNode}
			<dl class="space-y-1 text-caption">
				<div class="flex justify-between gap-2">
					<dt class="text-muted-foreground"
						>{t('domain.workflowGraphTab.selectedNode')}</dt
					>
					<dd class="truncate font-mono">{selectedNode.id}</dd>
				</div>
			</dl>
			{#if onedit}
				<div class="mt-2">
					<Button
						variant="outline"
						size="sm"
						onclick={() => onedit?.()}
					>
						<Icon name="pencil" size={13} />
						{t('domain.workflowGraphTab.openInEdit')}
					</Button>
				</div>
			{/if}
		{/if}
	{/snippet}
</module.default> {/await}

<div class="mt-3">
	<Card title={t('domain.workflowGraphTab.recentRuns')}>
		{#snippet actions()}
			<Button
				variant="ghost"
				size="sm"
				onclick={() => (logsOpen = !logsOpen)}
			>
				{logsOpen
					? t('domain.workflowGraphTab.hideRecentRuns')
					: t('domain.workflowGraphTab.showRecentRuns')}
			</Button>
		{/snippet}
		{#if logsOpen}
			<RunsPanel kind="workflow" definitionId={detail.id} />
		{/if}
	</Card>
</div>

<div class="mt-3 grid gap-3 lg:grid-cols-2">
	<Card title={t('domain.workflowGraphTab.nodes')}>
		<ul class="space-y-1.5">
			{#each detail.graph.nodes as node (node.id)}
				<li
					id={`wf-node-${node.id}`}
					class="flex items-center justify-between gap-2 text-caption"
				>
					<button
						type="button"
						class="truncate font-mono underline-offset-2 hover:underline"
						onclick={() => (graph.selection = node.id)}
					>
						{node.id}
					</button>
					<span class="flex shrink-0 items-center gap-2">
						<span class="text-muted-foreground">{node.kind}</span>
						<StatusBadge status={node.status} size="sm" dot={false} />
					</span>
				</li>
			{:else}
				<li class="text-caption text-muted-foreground">
					{t('domain.workflowGraphTab.noNodes')}
				</li>
			{/each}
		</ul>
	</Card>
	<Card title={t('domain.workflowGraphTab.edges')}>
		<ul class="space-y-1.5">
			{#each detail.graph.edges as edge (edge.id)}
				<li class="flex items-center gap-2 text-caption">
					<span class="font-mono">{edge.from}</span>
					<Icon name="arrow-right" size={12} class="text-muted-foreground" />
					<span class="font-mono">{edge.to}</span>
					{#if edge.label}
						<Badge variant="outline" class="text-[0.625rem]">{edge.label}</Badge
						>
					{/if}
				</li>
			{:else}
				<li class="text-caption text-muted-foreground">
					{t('domain.workflowGraphTab.noEdges')}
				</li>
			{/each}
		</ul>
	</Card>
</div>
