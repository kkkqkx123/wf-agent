<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import WorkflowGraph from '$lib/components/domain/WorkflowGraph.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import SessionList from '$lib/components/domain/SessionList.svelte';
	import ToolCallCard from '$lib/components/domain/ToolCallCard.svelte';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import ReasoningBlock from '$lib/components/chat/ReasoningBlock.svelte';
	import Composer from '$lib/components/chat/Composer.svelte';
	import { getAgentLoop, listAgentLoops } from '$lib/services/agent-loops';
	import { streamLoopRun } from '$lib/services/streaming';
	import { listCheckpointsByEntity } from '$lib/services/checkpoints';
	import { listModelProfiles } from '$lib/services/resources';
	import { removeFavorite, setFavorite } from '$lib/services/favorites';
	import type {
		AgentLoopDetail,
		Checkpoint,
		ToolCallEntry,
	} from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { chatStream } from '$lib/stores/chat-stream.svelte';
	import type { LiveToolCall } from '$lib/stores/chat-stream.svelte';
	import { preferences } from '$lib/stores/preferences.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber, formatRelativeTime } from '$lib/utils/format';
	import { appPath, gotoWithParams, parseListParams } from '$lib/utils/route';

	const PANEL_TABS = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'graph', label: 'Graph' },
		{ id: 'checkpoints', label: 'Checkpoints' },
	];

	const SUGGESTIONS = [
		'Summarize the current workspace state',
		'List the available tools and skills',
		'Draft a plan before making changes',
	];

	function isPanelTab(value: string | undefined): boolean {
		return PANEL_TABS.some((tab) => tab.id === value);
	}

	const initial = parseListParams(page.url);

	let query = $state(initial.q ?? '');
	let selectedId = $state<string | null>(initial.id ?? null);
	let panelTab = $state(
		initial.tab && isPanelTab(initial.tab) ? initial.tab : 'overview',
	);

	const list = createCollection((params) => listAgentLoops(params));
	const detail = createResource<AgentLoopDetail | null>(async () => {
		if (!selectedId) return null;
		return getAgentLoop(selectedId);
	});
	const checkpointList = createResource<Checkpoint[]>(async () => {
		if (!selectedId) return [];
		return listCheckpointsByEntity(selectedId);
	});
	const checkpoints = $derived(checkpointList.data ?? []);

	let model = $state('');
	let timeline: HTMLDivElement | null = $state(null);

	const showLive = $derived(
		chatStream.sessionKey === (selectedId ?? 'new') &&
			(chatStream.active ||
				chatStream.done ||
				chatStream.error !== null ||
				chatStream.answer !== ''),
	);

	function draftKey(): string {
		return `wf-chat-draft:${selectedId ?? 'new'}`;
	}

	function scrollTimeline(): void {
		if (!timeline) return;
		timeline.scrollTo({ top: timeline.scrollHeight });
	}

	function select(id: string): void {
		chatStream.stop();
		selectedId = id;
		const session = list.items.find((item) => item.id === id);
		ui.recordVisit(`/chat?id=${id}`, session?.name || id);
	}

	async function toggleStar(id: string, starred: boolean): Promise<void> {
		try {
			if (starred) await setFavorite('agent_loop', id, {});
			else await removeFavorite('agent_loop', id);
			await list.reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Star update failed');
		}
	}

	function startDraft(): void {
		chatStream.stop();
		selectedId = null;
	}

	function reloadActive(): void {
		void list.reload();
		if (selectedId) {
			void detail.reload();
			void checkpointList.reload();
		}
	}

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

	function adoptNewest(baseline: Set<string>, startedAt: string): void {
		const fresh = list.items
			.filter((item) => !baseline.has(item.id) && item.startedAt >= startedAt)
			.sort((a, b) => (a.startedAt < b.startedAt ? 1 : -1));
		const fallback = list.items
			.filter((item) => !baseline.has(item.id))
			.sort((a, b) => (a.updatedAt < b.updatedAt ? 1 : -1));
		const next = fresh[0] ?? fallback[0];
		if (next) selectedId = next.id;
	}

	function sendText(text: string): boolean {
		const content = text.trim();
		if (!content || chatStream.active) return false;
		if (!model.trim()) {
			toasts.error('Choose a model before sending');
			return false;
		}
		void runStream(content);
		return true;
	}

	function retryLast(): void {
		const messages = detail.data?.messages ?? [];
		for (let index = messages.length - 1; index >= 0; index -= 1) {
			if (messages[index].role === 'user') {
				sendText(messages[index].content);
				return;
			}
		}
		toasts.error('No user message to retry');
	}

	function recordFeedback(kind: 'up' | 'down'): void {
		if (kind === 'up') toasts.success('Thanks for the feedback');
		else toasts.info('Feedback recorded, will improve');
	}

	function runCommand(command: string): void {
		if (command === 'new') startDraft();
		else if (command === 'retry') retryLast();
		else if (command === 'continue') sendText('Continue');
	}

	async function runStream(content: string): Promise<void> {
		const sessionKey = selectedId ?? 'new';
		const baseline = new Set(list.items.map((item) => item.id));
		const startedAt = new Date().toISOString();
		const signal = chatStream.start(sessionKey);
		const history = (detail.data?.messages ?? []).map((message) => ({
			id: message.id,
			role: message.role,
			content: message.content,
			timestamp: Date.parse(message.createdAt) || Date.now(),
		}));
		scrollTimeline();
		await streamLoopRun(
			selectedId ?? 'new',
			{ model: model.trim(), message: content, conversation: history },
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
				onError: (message) => chatStream.fail(message),
			},
			signal,
		);
		if (!chatStream.error && !chatStream.done) chatStream.complete();
		await list.reload();
		if (!selectedId) adoptNewest(baseline, startedAt);
		if (selectedId) {
			await detail.reload();
			await checkpointList.reload();
		}
		scrollTimeline();
	}

	$effect(() => {
		if (selectedId) {
			void detail.reload();
			void checkpointList.reload();
		}
	});

	$effect(() => {
		gotoWithParams(page.url, {
			q: query,
			id: selectedId ?? '',
			tab: panelTab === 'overview' ? '' : panelTab,
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => {
		void list.loadPages(Number(initial.page) || 1);
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
	inspectorOpen={selectedId !== null}
	oninspectorclose={() => (panelTab = 'overview')}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col md:flex-row">
		<SessionList
			sessions={list.items}
			{selectedId}
			loading={list.loading}
			bind:query
			onselect={select}
			onnew={startDraft}
			onstar={(id, starred) => void toggleStar(id, starred)}
			class="max-h-52 shrink-0 border-b border-border md:max-h-none md:w-64 md:border-b-0 md:border-r"
		/>

		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			<div
				class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3"
			>
				<div class="min-w-0 flex-1">
					{#if detail.data}
						<div class="flex min-w-0 items-center gap-2">
							<h1 class="truncate text-title font-semibold">
								{detail.data.name || detail.data.id}
							</h1>
							<StatusBadge status={detail.data.status} size="sm" />
							<Badge variant="outline">
								iteration {detail.data.iteration}/{detail.data.maxIterations}
							</Badge>
						</div>
					{:else}
						<h1 class="truncate text-title font-semibold">New session</h1>
					{/if}
				</div>
				<IconButton icon="refresh" label="Refresh" onclick={reloadActive} />
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

			<div bind:this={timeline} class="min-h-0 flex-1 overflow-y-auto">
				<div
					class="mx-auto flex max-w-3xl flex-col gap-3 px-4 py-4 {preferences.chatFont ===
					'mono'
						? 'font-mono'
						: ''}"
				>
					{#if detail.loading && !detail.data && selectedId}
						<Skeleton shape="block" height="72px" class="rounded-lg" />
						<Skeleton shape="block" height="72px" class="rounded-lg" />
						<Skeleton shape="block" height="72px" class="rounded-lg" />
					{:else if detail.error}
						<ErrorState
							title="Failed to load session"
							description={detail.error}
							onretry={() => detail.reload()}
							class="rounded-lg border border-border bg-card"
						/>
					{:else if (detail.data?.messages.length ?? 0) === 0 && !selectedId}
						<EmptyState
							icon="sparkles"
							title="Start a conversation"
							description="Ask anything. The first send creates a tracked agent loop for this session."
							class="rounded-lg border border-border bg-card"
						/>
					{:else}
						{#each detail.data?.messages ?? [] as message (message.id)}
							<MessageBubble
								{message}
								onretry={retryLast}
								oncontinue={() => sendText('Continue')}
								onfeedback={recordFeedback}
							/>
						{/each}
						{#if (detail.data?.messages.length ?? 0) === 0 && !showLive}
							<p class="text-caption text-muted-foreground">
								No messages recorded for this session yet.
							</p>
						{/if}
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
								</div>
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
					{/if}
				</div>
			</div>

			<div class="shrink-0 border-t border-border px-3 py-2.5">
				<div class="mx-auto max-w-3xl">
					<Composer
						bind:model
						busy={chatStream.active}
						draftKey={draftKey()}
						suggestions={SUGGESTIONS}
						showSuggestions={(detail.data?.messages.length ?? 0) === 0}
						onsend={sendText}
						onstop={() => chatStream.stop()}
						oncommand={runCommand}
					/>
				</div>
			</div>
		</div>
	</div>

	{#snippet inspector()}
		<div class="flex h-full min-h-0 flex-col">
			<Segmented items={PANEL_TABS} bind:value={panelTab} class="px-3 pt-2" />
			<div class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
				{#if !detail.data}
					<p class="text-caption text-muted-foreground">
						Select a session to inspect its graph and checkpoints.
					</p>
				{:else if panelTab === 'graph'}
					<WorkflowGraph graph={detail.data.graph} class="max-h-80" />
					<Card title="Iterations" class="mt-3">
						{#if detail.data.iterations.length === 0}
							<p class="text-caption text-muted-foreground">
								No iteration records for this session.
							</p>
						{:else}
							<ul class="space-y-2">
								{#each detail.data.iterations as iteration (iteration.index)}
									<li
										class="flex items-start justify-between gap-3 border-b border-border/60 pb-2 last:border-0 last:pb-0"
									>
										<p class="min-w-0 text-caption">
											<span class="font-mono text-muted-foreground">
												#{iteration.index}
											</span>
											<span class="ml-2">{iteration.summary}</span>
										</p>
										<StatusBadge
											status={iteration.status}
											size="sm"
											dot={false}
										/>
									</li>
								{/each}
							</ul>
						{/if}
					</Card>
				{:else if panelTab === 'checkpoints'}
					{#if checkpointList.loading && !checkpointList.data}
						<div class="space-y-2">
							<Skeleton shape="block" height="96px" class="rounded-lg" />
							<Skeleton shape="block" height="96px" class="rounded-lg" />
						</div>
					{:else if checkpointList.error}
						<ErrorState
							title="Failed to load checkpoints"
							description={checkpointList.error}
							onretry={() => checkpointList.reload()}
						/>
					{:else if checkpoints.length === 0}
						<p class="text-caption text-muted-foreground">
							No checkpoints recorded for this session.
						</p>
					{:else}
						<div class="space-y-2">
							{#each checkpoints as checkpoint (checkpoint.id)}
								<Card title="{checkpoint.kind} · #{checkpoint.sequence}">
									{#snippet actions()}
										<Badge
											variant={checkpoint.restorable ? 'success' : 'neutral'}
										>
											{checkpoint.restorable ? 'restorable' : 'locked'}
										</Badge>
									{/snippet}
									<p class="text-caption text-muted-foreground">
										{checkpoint.note}
									</p>
								</Card>
							{/each}
						</div>
					{/if}
				{:else}
					<Card title="Run facts">
						<KeyValueList
							items={[
								{ key: 'model', value: detail.data.model },
								{ key: 'tokens', value: formatNumber(detail.data.tokens) },
								{
									key: 'checkpoints',
									value: formatNumber(detail.data.checkpoints),
								},
								{ key: 'errors', value: formatNumber(detail.data.errors) },
								{
									key: 'updated',
									value: formatRelativeTime(detail.data.updatedAt),
								},
							]}
							dense
						/>
					</Card>
					<Card title="Tags" class="mt-3">
						<div class="flex flex-wrap gap-1.5">
							{#each detail.data.tags as tag (tag)}
								<Badge variant="outline" size="sm">{tag}</Badge>
							{:else}
								<span class="text-caption text-muted-foreground">No tags</span>
							{/each}
						</div>
					</Card>
				{/if}
			</div>
		</div>
	{/snippet}
</SplitView>
