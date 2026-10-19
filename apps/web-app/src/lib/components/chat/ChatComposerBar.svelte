<script lang="ts">
	import Composer from '$lib/components/chat/Composer.svelte';
	import DecisionMap from '$lib/components/chat/DecisionMap.svelte';
	import { chatStream } from '$lib/stores/stream-run.svelte';
	import { NEW_SESSION } from '$lib/stores/sessions.svelte';
	import type { ChatSessionController } from '$lib/chat/chat-session.svelte';

	const SUGGESTIONS = [
		'Summarize the current workspace state',
		'List the available tools and skills',
		'Draft a plan before making changes',
	];

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const selectedId = $derived(controller.selectedId);
</script>

<div class="shrink-0 border-t border-border px-3 py-2.5">
	<DecisionMap {controller} />
	<div class="mx-auto max-w-3xl">
		<Composer
			bind:model={controller.model}
			bind:agent={controller.agentId}
			busy={chatStream.active}
			draftKey={selectedId ?? NEW_SESSION}
			suggestions={SUGGESTIONS}
			showSuggestions={controller.bubbles.length === 0}
			onsend={(message) => controller.sendText(message)}
			onstop={() => chatStream.stop()}
			oncommand={(action, arg) => controller.COMMANDS[action](arg)}
		/>
	</div>
</div>
