<script lang="ts">
	import type { WorkflowGraph } from '$lib/types/models';
	import GraphCanvas from '$lib/components/domain/GraphCanvas.svelte';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';

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

	const nodes = $derived<DisplayNode[]>(
		graph.nodes.map((node) => ({
			id: node.id,
			label: node.label,
			kind: node.kind,
			status: node.status,
		})),
	);

	const edges = $derived<DisplayEdge[]>(
		graph.edges.map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			label: edge.label,
		})),
	);
</script>

<GraphCanvas
	{nodes}
	{edges}
	preset="workflow"
	layout="layered"
	{selectedId}
	{onselect}
	class={className}
/>
