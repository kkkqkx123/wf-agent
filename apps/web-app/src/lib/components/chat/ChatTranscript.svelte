<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import ReasoningBlock from '$lib/components/chat/ReasoningBlock.svelte';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import ToolCallCard from '$lib/components/domain/ToolCallCard.svelte';
	import ToolCallGroup from '$lib/components/domain/ToolCallGroup.svelte';
	import TranscriptScroller from '$lib/components/chat/TranscriptScroller.svelte';
	import { chatStream } from '$lib/stores/stream-run.svelte';
	import { NEW_SESSION } from '$lib/stores/sessions.svelte';
	import { formatDuration, formatNumber } from '$lib/utils/format';
	import type { LoopMessage, ToolCallEntry } from '$lib/types/models';
	import type { ChatSessionController } from '$lib/chat/chat-session.svelte';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const bubbles = $derived(controller.bubbles);
	const selectedId = $derived(controller.selectedId);
	const showLive = $derived(controller.showLive);
	const showTranscript = $derived(controller.showTranscript);
	const liveGroups = $derived(controller.liveGroups);
	const groupTools = $derived(controller.groupTools);
</script>

{#if showTranscript}
	<TranscriptScroller
		items={bubbles}
		itemKey={(message: LoopMessage) => message.id}
		activity={controller.activity}
		resetKey={selectedId ?? NEW_SESSION}
		contentClass="mx-auto max-w-3xl px-4 py-4"
		class="min-h-0 flex-1"
	>
		{#snippet renderItem(message: LoopMessage)}
			<MessageBubble
				{message}
				onretry={() => controller.retryLast()}
				oncontinue={() => controller.sendPrompt('Continue')}
				onfeedback={(kind) => controller.recordFeedback(kind)}
			/>
		{/snippet}
		{#snippet tail()}
			{#if showLive}
				{#if chatStream.reasoning}
					<ReasoningBlock
						content={chatStream.reasoning}
						streaming={chatStream.active}
					/>
				{/if}
				{#snippet liveMember(entry: ToolCallEntry)}
					<div
						id={`live-tool-${entry.id}`}
						role="button"
						tabindex={0}
						aria-label={`Locate tool ${entry.name} on decision map`}
						onclick={() => controller.focusNodeFromTool(entry.id, entry.name)}
						onkeydown={(event) => {
							if (event.key === 'Enter' || event.key === ' ') {
								event.preventDefault();
								controller.focusNodeFromTool(entry.id, entry.name);
							}
						}}
						class={controller.focusedToolId === entry.id
							? 'cursor-pointer rounded-lg ring-2 ring-warning'
							: 'cursor-pointer rounded-lg'}
					>
						<ToolCallCard
							{entry}
							subagents={chatStream.subAgentsOf(entry.id)}
						/>
					</div>
				{/snippet}
				{#if groupTools}
					{#each liveGroups as group (group.key)}
						{#if group.kind === 'single'}
							{#each group.members as entry (entry.id)}
								{@render liveMember(entry)}
							{/each}
						{:else}
							<ToolCallGroup
								{group}
								open={controller.openGroups.has(group.key)}
								ontoggle={() => controller.toggleGroup(group.key)}
								member={liveMember}
							/>
						{/if}
					{/each}
				{:else}
					{#each chatStream.tools as tool (tool.id)}
						{@render liveMember(controller.toLiveEntry(tool))}
					{/each}
				{/if}
				{#if chatStream.answer}
					<article class="flex gap-2.5">
						<div
							class="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-border bg-muted text-muted-foreground"
						>
							<Icon name="sparkles" size={13} />
						</div>
						<div
							class="min-w-0 max-w-[min(46rem,88%)] rounded-lg border border-border bg-card px-3 py-2 text-card-foreground"
						>
							<StreamMarkdown
								content={chatStream.answer}
								done={!chatStream.active}
							/>
							{#if chatStream.usage}
								<p
									class="mt-1.5 border-t border-border/60 pt-1 text-micro tabular-nums text-muted-foreground"
								>
									{formatNumber(chatStream.usage.promptTokens)} prompt ·
									{formatNumber(chatStream.usage.completionTokens)}
									completion
									{#if chatStream.usage.cost !== null}
										· ${chatStream.usage.cost.toFixed(4)}
									{/if}
								</p>
							{/if}
						</div>
					</article>
				{/if}
				{#if chatStream.error}
					<div
						role="status"
						class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-caption text-destructive"
					>
						{chatStream.error}
						{#if chatStream.retryAfterMs !== null}
							<span class="mt-1 block text-muted-foreground">
								Rate limited — retry in
								{formatDuration(chatStream.retryAfterMs)}.
							</span>
						{/if}
					</div>
				{:else if chatStream.cancelled}
					<p role="status" class="text-caption text-muted-foreground">
						Run stopped before it finished.
					</p>
				{:else if chatStream.done}
					<p role="status" class="sr-only">Run completed.</p>
				{/if}
				{#if chatStream.active}
					<div class="flex items-center gap-2">
						<p
							class="flex items-center gap-2 text-caption text-muted-foreground"
						>
							<Icon name="loader" size={13} class="animate-spin" />
							Running{#if chatStream.iteration !== null}
								· iteration {chatStream.iteration}{/if}…
						</p>
						<Button
							variant="outline"
							size="sm"
							onclick={() => chatStream.stop()}
						>
							<Icon name="square" size={12} />
							Stop
						</Button>
					</div>
				{/if}
			{/if}
		{/snippet}
	</TranscriptScroller>
{:else}
	<div class="min-h-0 flex-1 overflow-y-auto">
		<div class="mx-auto flex max-w-3xl flex-col gap-3 px-4 py-4">
			{#if controller.transcript.loading && !controller.transcript.data && selectedId}
				<Skeleton shape="block" height="72px" class="rounded-lg" />
				<Skeleton shape="block" height="72px" class="rounded-lg" />
				<Skeleton shape="block" height="72px" class="rounded-lg" />
			{:else if controller.transcript.error}
				<ErrorState
					title="Failed to load session"
					description={controller.transcript.error}
					onretry={() => controller.transcript.reload()}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<EmptyState
					icon="sparkles"
					title={selectedId ? 'No messages yet' : 'Start a conversation'}
					description={selectedId
						? 'This session has no recorded messages.'
						: 'Ask anything. The first send creates a tracked agent loop for this session.'}
					class="rounded-lg border border-border bg-card"
				/>
			{/if}
		</div>
	</div>
{/if}
