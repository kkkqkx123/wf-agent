<script lang="ts">
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import GraphCanvas from '$lib/components/domain/GraphCanvas.svelte';
	import { chatStream } from '$lib/stores/stream-run.svelte';
	import { t } from '$lib/i18n';
	import type { ChatSessionController } from '$lib/chat/chat-session.svelte';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const selectedId = $derived(controller.selectedId);
	const mapOpen = $derived(controller.mapOpen);
	const miniBaseNodes = $derived(controller.miniBaseNodes);

	let canvas: { zoomTo: (id: string) => void } | null = $state(null);

	// Mirror the canvas handle onto the controller so tool focus can zoom.
	$effect(() => {
		controller.miniCanvas = canvas;
	});
</script>

{#if selectedId}
	<div class="mx-auto mb-2 max-w-3xl">
		<button
			type="button"
			class="text-micro text-muted-foreground underline-offset-2 hover:underline"
			onclick={() => (controller.mapOpen = !controller.mapOpen)}
		>
			{mapOpen ? t('chat.decisionMap.hide') : t('chat.decisionMap.show')}
			{chatStream.tools.length > 0
				? ` · ${t('chat.decisionMap.liveTools', { count: chatStream.tools.length })}`
				: ''}
		</button>
		{#if mapOpen}
			<div class="mt-1.5">
				{#if controller.loopGraph.loading && !controller.loopGraph.data}
					<Skeleton shape="block" height="120px" class="rounded-lg" />
				{:else if miniBaseNodes.length === 0}
					<p class="text-micro text-muted-foreground">
						{t('chat.decisionMap.noGraph')}
					</p>
				{:else}
					<GraphCanvas
						bind:this={canvas}
						nodes={controller.miniNodes}
						edges={controller.miniEdges}
						preset="decision"
						layout="columns"
						selectedId={controller.miniSelected}
						onselect={(nodeId: string) => controller.focusToolFromNode(nodeId)}
						heightClass="h-56"
					/>
				{/if}
			</div>
		{/if}
	</div>
{/if}
