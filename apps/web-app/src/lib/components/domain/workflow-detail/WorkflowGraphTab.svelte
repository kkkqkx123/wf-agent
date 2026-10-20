<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
		import type { WorkflowDetail } from '$lib/types/models';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import type { WorkflowGraphState } from '$lib/workflows/graph-state.svelte';
	import { t } from '$lib/i18n';

	interface Props {
		detail: WorkflowDetail;
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		state: WorkflowGraphState;
	}

	// Selection, overlays and analysis live in the route-owned state object so
	// leaving the tab and returning keeps them; the route reloads it with the
	// detail (load/promotion), matching the pre-split page behavior.
	let { detail, nodes, edges, state }: Props = $props();
</script>

{#if state.analysisError}
	<p class="mb-2 text-caption text-warning">
		{t('domain.workflowGraphTab.analysisUnavailable', {
			error: state.analysisError,
		})}
		<button
			type="button"
			class="underline"
			onclick={() => void state.loadAnalysis(detail.id)}
		>
			{t('domain.hierarchyTree.retry')}
		</button>.
	</p>
{/if}
{#await import('$lib/components/domain/GraphExplorer.svelte') then module}
<module.default
	{nodes}
	{edges}
	preset="workflow"
	layoutId={detail.id}
	selectedId={state.selection}
	onselect={(id) => (state.selection = id)}
	onexpand={(id) => void state.expandNeighborhood(detail.id, id)}
	expandLabel={t('domain.inspector.revealNeighborhood')}
	overlays={state.effectiveOverlays}
	activeOverlay={state.activeOverlay}
	onoverlay={(id) => (state.activeOverlay = id)}
	onjumpparam={(id) => state.jumpToNodeRow(id)}
/> {/await}
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
						onclick={() => (state.selection = node.id)}
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
