<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import GraphExplorer, {
		type GraphOverlay,
	} from '$lib/components/domain/GraphExplorer.svelte';
	import { getGraphAnalysis, getGraphNeighbors } from '$lib/services/graph';
	import type { GraphAnalysisResult, WorkflowDetail } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';

	interface Props {
		detail: WorkflowDetail;
		workflowId: string;
		nodes: DisplayNode[];
		edges: DisplayEdge[];
	}

	let { detail, workflowId, nodes, edges }: Props = $props();

	let graphNodeId = $state<string | null>(null);
	let analysis = $state<GraphAnalysisResult | null>(null);
	let analysisError = $state<string | null>(null);
	let activeOverlay = $state<string | null>(null);
	let neighborhoodIds = $state<string[]>([]);

	// Graph state is component-local; a workflow switch clears the selection
	// and re-runs the analysis, as the page-level load() used to do.
	const detailKey = $derived(detail.id);
	$effect(() => {
		graphNodeId = null;
		activeOverlay = null;
		void loadAnalysis(detailKey);
	});

	async function loadAnalysis(id: string): Promise<void> {
		analysis = null;
		analysisError = null;
		try {
			analysis = await getGraphAnalysis(id);
		} catch (e) {
			analysisError =
				e instanceof Error ? e.message : 'Analysis failed to load.';
		}
	}

	async function expandNeighborhood(id: string): Promise<void> {
		if (!workflowId) return;
		try {
			const neighbors = await getGraphNeighbors(workflowId, id);
			neighborhoodIds = [
				id,
				...neighbors.predecessors,
				...neighbors.successors,
			];
			activeOverlay = '__neighborhood';
			toasts.success(
				`Neighborhood: ${neighbors.predecessors.length} in · ${neighbors.successors.length} out`,
			);
		} catch (e) {
			toasts.error(
				'Neighborhood failed to load',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	/** Jump from the graph detail card to the node row in the Nodes list. */
	function jumpToNodeRow(id: string): void {
		graphNodeId = id;
		requestAnimationFrame(() => {
			document
				.getElementById(`wf-node-${id}`)
				?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
		});
	}

	const overlays = $derived.by<GraphOverlay[]>(() => {
		if (!analysis) return [];
		const list: GraphOverlay[] = [];
		if (analysis.cycleDetection.hasCycle) {
			list.push({
				id: 'cycles',
				label: 'Cycle nodes',
				ids: analysis.cycleDetection.cycleNodes,
			});
		}
		if (analysis.reachability.unreachableNodes.length > 0) {
			list.push({
				id: 'unreachable',
				label: 'Unreachable',
				ids: analysis.reachability.unreachableNodes,
			});
		}
		if (analysis.reachability.deadEndNodes.length > 0) {
			list.push({
				id: 'dead-ends',
				label: 'Dead ends',
				ids: analysis.reachability.deadEndNodes,
			});
		}
		return list;
	});

	const effectiveOverlays = $derived.by<GraphOverlay[]>(() => {
		if (activeOverlay === '__neighborhood') {
			return [
				...overlays,
				{ id: '__neighborhood', label: 'Neighborhood', ids: neighborhoodIds },
			];
		}
		return overlays;
	});
</script>

{#if analysisError}
	<p class="mb-2 text-caption text-warning">
		Graph analysis unavailable ({analysisError});
		<button
			type="button"
			class="underline"
			onclick={() => void loadAnalysis(detail.id)}
		>
			retry
		</button>.
	</p>
{/if}
<GraphExplorer
	{nodes}
	{edges}
	preset="workflow"
	selectedId={graphNodeId}
	onselect={(id) => (graphNodeId = id)}
	onexpand={(id) => void expandNeighborhood(id)}
	expandLabel="Reveal neighborhood"
	overlays={effectiveOverlays}
	{activeOverlay}
	onoverlay={(id) => (activeOverlay = id)}
	onjumpparam={jumpToNodeRow}
/>
<div class="mt-3 grid gap-3 lg:grid-cols-2">
	<Card title="Nodes">
		<ul class="space-y-1.5">
			{#each detail.graph.nodes as node (node.id)}
				<li
					id={`wf-node-${node.id}`}
					class="flex items-center justify-between gap-2 text-caption"
				>
					<button
						type="button"
						class="truncate font-mono underline-offset-2 hover:underline"
						onclick={() => (graphNodeId = node.id)}
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
					This definition has no nodes yet.
				</li>
			{/each}
		</ul>
	</Card>
	<Card title="Edges">
		<ul class="space-y-1.5">
			{#each detail.graph.edges as edge (edge.id)}
				<li class="flex items-center gap-2 text-caption">
					<span class="font-mono">{edge.from}</span>
					<Icon name="arrow-right" size={12} class="text-muted-foreground" />
					<span class="font-mono">{edge.to}</span>
					{#if edge.label}
						<Badge variant="outline" class="text-[0.625rem]"
							>{edge.label}</Badge
						>
					{/if}
				</li>
			{:else}
				<li class="text-caption text-muted-foreground">
					This definition has no edges yet.
				</li>
			{/each}
		</ul>
	</Card>
</div>
