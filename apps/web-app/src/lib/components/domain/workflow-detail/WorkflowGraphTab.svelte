<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import type { WorkflowDetail } from '#lib/types/models';
	import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
	import type { WorkflowGraphState } from '#lib/workflows/graph-state.svelte';
	import {
		applyEdgeOverlay,
		applyExecutionOverlay,
		canvasPropsFromOverlay,
		executionInputFromSignals,
		hasExecutionSignals,
		projectEdgeOverlay,
		projectExecutionOverlay,
	} from '#lib/graph/execution-projection';
	import RunsPanel from '#lib/components/domain/RunsPanel.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		detail: WorkflowDetail;
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		state: WorkflowGraphState;
		/** Enter the edit tab with the current selection. */
		onedit?: () => void;
		/** Clear the execution overlay; the host also drops `?run=` so a
		 * dismissed deep link does not reload on the next effect run. */
		onclearoverlay?: () => void;
	}

	// Selection, overlays and analysis live in the route-owned state object so
	// leaving the tab and returning keeps them; the route reloads it with the
	// detail (load/promotion), matching the pre-split page behavior.
	let { detail, nodes, edges, state: graph, onedit, onclearoverlay }: Props =
		$props();

	// The log strip shares the tab so graph context and recent runs stay
	// visible together; collapsed by default to preserve canvas height.
	let logsOpen = $state(false);

	// Read-only execution projection: the latest run is painted onto the
	// definition topology through the shared overlay helpers, never by
	// mutating the definition itself.
	const overlayInput = $derived(
		executionInputFromSignals(graph.executionOverlay),
	);
	const definitionOverlay = $derived(
		projectExecutionOverlay(nodes, overlayInput),
	);
	const overlayNodes = $derived(
		applyExecutionOverlay(nodes, definitionOverlay),
	);
	const overlayEdges = $derived.by(() =>
		applyEdgeOverlay(edges, projectEdgeOverlay(edges, definitionOverlay)),
	);
	const overlayProps = $derived(canvasPropsFromOverlay(definitionOverlay));
	const overlayActive = $derived(
		graph.executionOverlay !== null && hasExecutionSignals(overlayInput),
	);

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
	<div class="mb-2 flex flex-wrap items-center gap-2">
		{#if overlayActive && graph.overlayRunId}
			<Badge variant="info">Run {graph.overlayRunId.slice(0, 8)}</Badge>
			<Button
				variant="ghost"
				size="sm"
				href={`/executions/${graph.overlayRunId}`}
			>
				View execution
			</Button>
			<Button
				variant="ghost"
				size="sm"
				onclick={() => {
					if (onclearoverlay) onclearoverlay();
					else graph.clearExecutionOverlay();
				}}
			>
				Hide run overlay
			</Button>
		{:else}
			<Button
				variant="outline"
				size="sm"
				disabled={graph.overlayLoading}
				onclick={() => void graph.loadLatestRunOverlay(detail.id)}
			>
				{graph.overlayLoading ? 'Loading latest run…' : 'Show latest run'}
			</Button>
		{/if}
		{#if graph.overlayError}
			<span class="text-micro text-muted-foreground">{graph.overlayError}</span>
		{/if}
	</div>
	<module.default
		nodes={overlayActive ? overlayNodes : nodes}
		edges={overlayActive ? overlayEdges : edges}
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
		pulseIds={overlayProps.pulseIds}
		criticalIds={overlayProps.criticalIds}
		failedIds={overlayProps.failedIds}
		heatTierById={overlayProps.heatTierById}
		decisionIds={overlayProps.decisionIds}
	>
		{#snippet inspector()}
			{#if selectedNode}
				<dl class="space-y-1 text-caption">
					<div class="flex justify-between gap-2">
						<dt class="text-muted-foreground">
							{t('domain.workflowGraphTab.selectedNode')}
						</dt>
						<dd class="truncate font-mono">{selectedNode.id}</dd>
					</div>
				</dl>
				{#if onedit}
					<div class="mt-2">
						<Button variant="outline" size="sm" onclick={() => onedit?.()}>
							<Icon name="pencil" size={13} />
							{t('domain.workflowGraphTab.openInEdit')}
						</Button>
					</div>
				{/if}
			{/if}
		{/snippet}
	</module.default>
{/await}

<div class="mt-3">
	<Card title={t('domain.workflowGraphTab.recentRuns')}>
		{#snippet actions()}
			<Button variant="ghost" size="sm" onclick={() => (logsOpen = !logsOpen)}>
				{logsOpen
					? t('domain.workflowGraphTab.hideRecentRuns')
					: t('domain.workflowGraphTab.showRecentRuns')}
			</Button>
		{/snippet}
		{#if logsOpen}
			<RunsPanel kind="workflow" definitionId={detail.id} compact />
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
