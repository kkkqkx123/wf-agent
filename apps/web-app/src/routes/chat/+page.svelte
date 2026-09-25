<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import SessionInspector from '$lib/components/domain/SessionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import ToolCallCard from '$lib/components/domain/ToolCallCard.svelte';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import ReasoningBlock from '$lib/components/chat/ReasoningBlock.svelte';
	import Composer from '$lib/components/chat/Composer.svelte';
	import TranscriptScroller from '$lib/components/chat/TranscriptScroller.svelte';
	import {
		listLoopMessages,
		type RunLoopMessage,
	} from '$lib/services/agent-loops';
	import { streamLoopRun } from '$lib/services/streaming';
	import { listModelProfiles } from '$lib/services/resources';
	import type { CommandAction } from '$lib/config/commands';
	import type {
		LoopMessage,
		MessageAttachment,
		OutgoingMessage,
		ToolCallEntry,
	} from '$lib/types/models';
	import { splitAttachments, withAttachments } from '$lib/utils/attachments';
	import { createResource } from '$lib/stores/collection.svelte';
	import { chatStream } from '$lib/stores/stream-run.svelte';
	import type { LiveToolCall } from '$lib/stores/stream-run.svelte';
	import { preferences } from '$lib/stores/preferences.svelte';
	import { NEW_SESSION, sessions } from '$lib/stores/sessions.svelte';
	import { isSessionTab, type SessionTab } from '$lib/config/session-tabs';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration, formatNumber } from '$lib/utils/format';
	import { appPath, gotoWithParams, parseListParams } from '$lib/utils/route';

	const SUGGESTIONS = [
		'Summarize the current workspace state',
		'List the available tools and skills',
		'Draft a plan before making changes',
	];

	/** Selection lives in the address, so the sidebar and this page share it. */
	const selectedId = $derived(page.url.searchParams.get('id'));

	let model = $state('');
	let revision = $state(0);

	const initial = parseListParams(page.url);
	let tab = $state<SessionTab>(isSessionTab(initial.tab) ?? 'overview');
	let panelOpen = $state(initial.panel !== 'closed');

	const transcript = createResource<LoopMessage[]>(async () => {
		if (!selectedId) return [];
		return (await listLoopMessages(selectedId)).items;
	});

	const bubbles = $derived(transcript.data ?? []);

	/** Content counter the transcript viewport follows while pinned to the tail. */
	const activity = $derived(
		bubbles.length +
			chatStream.answer.length +
			chatStream.reasoning.length +
			chatStream.tools.length,
	);

	const showLive = $derived(
		chatStream.sessionKey === (selectedId ?? NEW_SESSION) &&
			(chatStream.active ||
				chatStream.done ||
				chatStream.cancelled ||
				chatStream.error !== null ||
				chatStream.answer !== ''),
	);

	const showTranscript = $derived(bubbles.length > 0 || showLive);

	const session = $derived(
		selectedId
			? (sessions.list.items.find((item) => item.id === selectedId) ?? null)
			: null,
	);

	function toLiveEntry(tool: LiveToolCall): ToolCallEntry {
		return {
			id: tool.id,
			name: tool.name,
			kind: '',
			status: tool.status,
			startedAt: '',
			durationMs: 0,
			input: '',
			output: tool.result,
		};
	}

	function sendText(message: OutgoingMessage): boolean {
		const text = message.text.trim();
		if (!text && message.attachments.length === 0) return false;
		if (chatStream.active) return false;
		if (!model.trim()) {
			toasts.error('Choose a model before sending');
			return false;
		}
		void runStream(text, message.attachments);
		return true;
	}

	function sendPrompt(text: string): boolean {
		return sendText({ text, attachments: [] });
	}

	function retryLast(): void {
		for (let index = bubbles.length - 1; index >= 0; index -= 1) {
			if (bubbles[index].role === 'user') {
				sendText(splitAttachments(bubbles[index].content));
				return;
			}
		}
		toasts.error('No user message to retry');
	}

	function recordFeedback(kind: 'up' | 'down'): void {
		if (kind === 'up') toasts.success('Thanks for the feedback');
		else toasts.info('Feedback recorded, will improve');
	}

	/** Every slash command maps to one action here; the table owns the vocabulary. */
	const COMMANDS: Record<CommandAction, (arg: string) => void> = {
		new: () => sessions.startDraft(),
		retry: () => retryLast(),
		continue: (arg) => sendPrompt(arg ? `Continue: ${arg}` : 'Continue'),
	};

	async function runStream(
		content: string,
		files: MessageAttachment[],
	): Promise<void> {
		const draft = !selectedId;
		const baseline = new Set(sessions.list.items.map((item) => item.id));
		const startedAt = new Date().toISOString();
		const history: RunLoopMessage[] = bubbles.map((message) => ({
			id: message.id,
			role: message.role,
			content: message.content,
			timestamp: Date.parse(message.createdAt) || Date.now(),
		}));
		const signal = chatStream.start(selectedId ?? NEW_SESSION);
		await streamLoopRun(
			selectedId ?? NEW_SESSION,
			{
				model: model.trim(),
				message: withAttachments(content, files),
				conversation: history,
			},
			{
				onDelta: (delta) => chatStream.appendDelta(delta),
				onReasoning: (delta) => chatStream.appendReasoning(delta),
				onIterationStart: (iteration) => {
					chatStream.iteration = iteration;
				},
				onIterationEnd: (iteration) => {
					chatStream.iteration = iteration;
				},
				onToolStart: (toolCallId, toolName) =>
					chatStream.toolStart(toolCallId, toolName),
				onToolEnd: (tool) =>
					chatStream.toolEnd(
						tool.toolCallId,
						tool.toolName,
						tool.success,
						tool.result,
					),
				onUsage: (usage) => {
					chatStream.usage = usage;
				},
				onSubAgent: (id, name, success) =>
					chatStream.noteSubAgent(id, name, success),
				onCompleted: () => chatStream.complete(),
				onFailed: (message) => chatStream.fail(message),
				onInterrupted: (reason) => chatStream.fail(reason),
				onError: (failure) =>
					chatStream.fail(failure.message, failure.retryAfterMs),
			},
			signal,
		);
		if (!chatStream.error && !chatStream.done) chatStream.complete();
		await sessions.refresh();
		if (draft) {
			const fresh = sessions.sessions.filter((row) => !baseline.has(row.id));
			const adopted =
				fresh.find((row) => row.startedAt >= startedAt) ?? fresh[0];
			if (adopted) {
				sessions.recordTitle(adopted.id, content);
				sessions.open(adopted.id);
			}
		}
		revision += 1;
		if (selectedId) await transcript.reload();
	}

	// The address is the source of truth for which session is open.
	$effect(() => {
		const id = selectedId;
		if (!id) {
			transcript.data = [];
			return;
		}
		sessions.touch(id);
		void transcript.reload();
	});

	$effect(() => {
		gotoWithParams(page.url, {
			id: selectedId ?? '',
			tab: tab === 'overview' ? '' : tab,
			panel: panelOpen ? '' : 'closed',
		});
	});

	onMount(() => {
		void listModelProfiles()
			.then((profiles) => {
				const preferred =
					profiles.find((item) => item.isDefault) ?? profiles[0];
				if (preferred) model = preferred.model;
			})
			.catch(() => {
				// Profiles are optional; the composer keeps a manual input.
			});
	});
</script>

<SplitView
	inspectorTitle="Session panel"
	inspectorOpen={panelOpen && selectedId !== null}
	oninspectorclose={() => (panelOpen = false)}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col">
		<div
			class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3"
		>
			<div class="min-w-0 flex-1">
				{#if session}
					<div class="flex min-w-0 items-center gap-2">
						<h1 class="truncate text-title font-semibold">
							{sessions.label(session.id)}
						</h1>
						<StatusBadge status={session.status} size="sm" />
						<Badge variant="outline">
							iteration {formatNumber(session.iteration)}
						</Badge>
					</div>
				{:else}
					<h1 class="truncate text-title font-semibold">New session</h1>
				{/if}
			</div>
			{#if selectedId}
				<IconButton
					icon="star"
					label={sessions.isStarred(selectedId)
						? 'Unstar session'
						: 'Star session'}
					onclick={() => void sessions.toggleStar(selectedId)}
				/>
			{/if}
			<IconButton
				icon="refresh"
				label="Refresh session"
				onclick={() => {
					revision += 1;
					void transcript.reload();
				}}
			/>
			{#if selectedId}
				<Button
					variant="outline"
					size="sm"
					href={appPath(`/agent-loops/${selectedId}`)}
				>
					<Icon name="arrow-right" size={13} />
					Full detail
				</Button>
			{/if}
		</div>

		{#if showTranscript}
			<TranscriptScroller
				items={bubbles}
				itemKey={(message) => message.id}
				{activity}
				resetKey={selectedId ?? NEW_SESSION}
				contentClass="mx-auto max-w-3xl px-4 py-4 {preferences.chatFont ===
				'mono'
					? 'font-mono'
					: ''}"
				class="min-h-0 flex-1"
			>
				{#snippet renderItem(message: LoopMessage)}
					<MessageBubble
						{message}
						onretry={retryLast}
						oncontinue={() => sendPrompt('Continue')}
						onfeedback={recordFeedback}
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
						{#each chatStream.tools as tool (tool.id)}
							<ToolCallCard entry={toLiveEntry(tool)} />
						{/each}
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
							<p class="text-caption text-muted-foreground">
								Run stopped before it finished.
							</p>
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
					{#if transcript.loading && !transcript.data && selectedId}
						<Skeleton shape="block" height="72px" class="rounded-lg" />
						<Skeleton shape="block" height="72px" class="rounded-lg" />
						<Skeleton shape="block" height="72px" class="rounded-lg" />
					{:else if transcript.error}
						<ErrorState
							title="Failed to load session"
							description={transcript.error}
							onretry={() => transcript.reload()}
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

		<div class="shrink-0 border-t border-border px-3 py-2.5">
			<div class="mx-auto max-w-3xl">
				<Composer
					bind:model
					busy={chatStream.active}
					draftKey={selectedId ?? NEW_SESSION}
					suggestions={SUGGESTIONS}
					showSuggestions={bubbles.length === 0}
					onsend={sendText}
					onstop={() => chatStream.stop()}
					oncommand={(action, arg) => COMMANDS[action](arg)}
				/>
			</div>
		</div>
	</div>

	{#snippet inspector()}
		<SessionInspector
			sessionId={selectedId ?? ''}
			bind:tab
			{revision}
			busy={chatStream.active}
			class="h-full"
		/>
	{/snippet}
</SplitView>
