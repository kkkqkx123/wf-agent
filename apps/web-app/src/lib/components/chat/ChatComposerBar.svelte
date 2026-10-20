<script lang="ts">
	import Composer from '$lib/components/chat/Composer.svelte';
	import DecisionMap from '$lib/components/chat/DecisionMap.svelte';
	import { chatStream } from '$lib/stores/stream-run.svelte';
	import { NEW_SESSION } from '$lib/stores/sessions.svelte';
	import { t, type MessageKey } from '$lib/i18n';
	import type { ChatSessionController } from '$lib/chat/chat-session.svelte';

	// Suggestion keys resolve through `t()` so the locale switch applies live.
	const SUGGESTION_KEYS = [
		'chat.chatComposerBar.suggestSummary',
		'chat.chatComposerBar.suggestTools',
		'chat.chatComposerBar.suggestPlan',
	] as const satisfies readonly MessageKey[];

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const selectedId = $derived(controller.selectedId);
	const suggestions = $derived(SUGGESTION_KEYS.map((key) => t(key)));
</script>

<div class="shrink-0 border-t border-border px-3 py-2.5">
	<DecisionMap {controller} />
	<div class="mx-auto max-w-3xl">
		<Composer
			bind:model={controller.model}
			bind:agent={controller.agentId}
			bind:workflow={controller.workflowId}
			bind:executionType={controller.executionType}
			busy={chatStream.active}
			draftKey={selectedId ?? NEW_SESSION}
			suggestions={suggestions}
			showSuggestions={controller.bubbles.length === 0}
			onsend={(message) => controller.sendText(message)}
			onstop={() => chatStream.stop()}
			oncommand={(action, arg) => controller.COMMANDS[action](arg)}
		/>
	</div>
</div>
