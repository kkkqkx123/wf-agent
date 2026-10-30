<script lang="ts">
	import Composer from '#lib/components/chat/Composer.svelte';
	import DecisionMap from '#lib/components/chat/DecisionMap.svelte';
	import { chatStream } from '#lib/stores/stream-run.svelte';
	import { NEW_SESSION } from '#lib/stores/sessions.svelte';
	import { getLocale, t, type MessageKey } from '#lib/i18n';
	import {
		listStarters,
		recordStarterUsage,
		toSuggestion,
		type StarterSuggestion,
	} from '#lib/services/starters';
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';

	// Fallback chips when the backend is unreachable or returns nothing.
	// Ids are absent, so no usage is recorded for fallback picks.
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
	const locale = $derived(getLocale());

	let starters = $state<StarterSuggestion[] | null>(null);
	let backendOk = $state(true);
	let request = 0;

	/** Backend chips for the active locale; null while loading, so the
	 *  composer renders nothing instead of a layout-shifting skeleton.
	 *  An unreachable backend keeps the i18n fallback (older servers
	 *  lack the endpoint); an intentionally empty backend hides the row. */
	async function loadStarters(activeLocale: string, attempt: number): Promise<void> {
		try {
			const items = await listStarters({ locale: activeLocale });
			if (attempt !== request) return;
			starters = items.map(toSuggestion);
			backendOk = true;
		} catch {
			if (attempt !== request) return;
			starters = [];
			backendOk = false;
		}
	}

	$effect(() => {
		request += 1;
		starters = null;
		void loadStarters(locale, request);
	});

	// A locale switch re-renders `t()`, so fallback chips follow live.
	const fallback = $derived(SUGGESTION_KEYS.map((key) => ({ title: t(key), content: t(key) })));
	const suggestions = $derived(
		starters === null ? [] : starters.length > 0 || backendOk ? starters : fallback,
	);

	/** Starter awaiting usage credit; cleared on session switch. */
	let pendingStarterId = $state<string | null>(null);

	$effect(() => {
		void selectedId;
		pendingStarterId = null;
	});

	function handleSend(message: Parameters<ChatSessionController['sendText']>[0]): boolean {
		const accepted = controller.sendText(message);
		if (accepted && pendingStarterId) void recordStarterUsage(pendingStarterId);
		pendingStarterId = null;
		return accepted;
	}
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
			{suggestions}
			showSuggestions={controller.bubbles.length === 0}
			onsend={handleSend}
			onstop={() => chatStream.stop()}
			oncommand={(action, arg) => controller.COMMANDS[action](arg)}
			onSuggestionSelect={(suggestion) => {
				pendingStarterId = suggestion.id ?? null;
			}}
		/>
	</div>
</div>
