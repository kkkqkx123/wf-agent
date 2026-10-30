<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import SplitView from '#lib/components/layout/SplitView.svelte';
	import ChatSessionHeader from '#lib/components/chat/ChatSessionHeader.svelte';
	import ChatTranscript from '#lib/components/chat/ChatTranscript.svelte';
	import ChatComposerBar from '#lib/components/chat/ChatComposerBar.svelte';
	import ChatSessionRail from '#lib/components/chat/ChatSessionRail.svelte';
	import ChatInspectorPanel from '#lib/components/chat/ChatInspectorPanel.svelte';
	import { ChatSessionController } from '#lib/chat/chat-session.svelte';
	import { parseListParams } from '#lib/utils/route';

	const initial = parseListParams(page.url as unknown as URL);
	const controller = new ChatSessionController(initial.tab);
	let panelOpen = $state(initial.panel !== 'closed');
	let composerExpanded = $state(false);

	const selectedId = $derived(controller.selectedId);
	const tab = $derived(controller.tab);

	$effect(() => {
		controller.syncAddress(tab, panelOpen);
	});

	onMount(() => {
		controller.loadDefaultModel();
		controller.loadDefaultAgent();
	});
</script>

<SplitView
	inspectorTitle="Session panel"
	inspectorOpen={panelOpen && selectedId !== null}
	oninspectorclose={() => (panelOpen = false)}
	class="h-full"
>
	<div class="flex h-full min-h-0">
		<ChatSessionRail {controller} />
		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			<ChatSessionHeader {controller} />
			{#if !composerExpanded}
				<ChatTranscript {controller} />
			{/if}
			<ChatComposerBar
				{controller}
				bind:fullscreen={composerExpanded}
				onToggleFullscreen={() => (composerExpanded = !composerExpanded)}
			/>
		</div>
	</div>

	{#snippet inspector()}
		<ChatInspectorPanel {controller} />
	{/snippet}
</SplitView>
