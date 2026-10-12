<script lang="ts">
	import { getAgentLoopMessages } from '$lib/services/agent-loops';
	import type { LoopMessage } from '$lib/types/models';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';

	/** Transcript of one agent loop run; owns its own loading keyed by id. */
	let { id }: { id: string } = $props();

	let messages = $state<LoopMessage[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(false);
	let seen = $state('');

	$effect(() => {
		const loopId = id;
		if (seen === loopId) return;
		seen = loopId;
		void load(loopId);
	});

	async function load(loopId: string): Promise<void> {
		error = null;
		loading = true;
		try {
			messages = await getAgentLoopMessages(loopId);
		} catch (e) {
			seen = '';
			messages = [];
			error = e instanceof Error ? e.message : 'Messages failed.';
		} finally {
			loading = false;
		}
	}
</script>

<PageState
	{loading}
	{error}
	errorTitle="Messages failed to load"
	empty={messages.length === 0}
	emptyTitle="No messages recorded"
	emptyDescription="Loop messages appear here once the run exchanges prompts with the model."
	onretry={() => void load(id)}
	class="rounded-lg border border-border bg-card"
>
	<div class="mx-auto flex max-w-3xl flex-col gap-3">
		{#each messages as message (message.id)}
			<MessageBubble {message} />
		{/each}
	</div>
</PageState>
