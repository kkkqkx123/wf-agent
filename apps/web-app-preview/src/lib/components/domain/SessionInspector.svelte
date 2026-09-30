<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import TranscriptScroller from '$lib/components/chat/TranscriptScroller.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import Timeline from '$lib/components/domain/Timeline.svelte';
	import TimelineOutline from '$lib/components/domain/TimelineOutline.svelte';
	import ToolCallCard from '$lib/components/domain/ToolCallCard.svelte';
	import GraphExplorer from '$lib/components/domain/GraphExplorer.svelte';
	import {
		applyExecutionOverlay,
		matchDecisionNodeId,
		projectExecutionOverlay,
	} from '$lib/graph/execution-projection';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import type { Column } from '@wf-agent/ui/components/table';
	import {
		getAgentLoop,
		getLoopGraph,
		listLoopMessages,
		listLoopIterations,
		listLoopTimeline,
		listLoopVariables,
	} from '$lib/services/agent-loops';
	import { listLoopCheckpoints } from '$lib/services/checkpoints';
	import type {
		Checkpoint,
		LoopMessage,
		LoopVariable,
	} from '$lib/types/models';
	import {
		INSPECTOR_TABS,
		TAB_DATA,
		TAB_LABELS,
		type SessionTab,
		type TabDataKey,
	} from '$lib/config/session-tabs';
	import { createResource } from '$lib/stores/collection.svelte';
	import {
		formatBytes,
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '$lib/utils/format';
	import { cn } from '@wf-agent/ui/cn';

	interface TabState {
		loading: boolean;
		error: string | null;
		data: unknown;
		reload: () => Promise<void>;
	}

	interface Props {
		sessionId: string;
		tab?: SessionTab;
		/** Which tabs to offer; the detail page adds the transcript. */
		tabs?: SessionTab[];
		/** Bumping this refetches the sources already loaded for the session. */
		revision?: number;
		busy?: boolean;
		/** Live tool statuses keyed by tool name; merged over history. */
		liveStatuses?: Record<string, string>;
		/** Node id pulsed as currently running. */
		liveNode?: string | null;
		/** Focusing a node from outside (tool card clicks). */
		focusId?: string | null;
		/** Bubble graph selection up (tool list filtering). */
		onnodeselect?: (id: string | null) => void;
		/** When absent the checkpoint list stays read-only. */
		onrestore?: (checkpoint: Checkpoint) => void;
		class?: string;
	}

	let {
		sessionId,
		tab = $bindable('overview'),
		tabs = INSPECTOR_TABS,
		revision = 0,
		busy = false,
		liveStatuses = {},
		liveNode = null,
		focusId = null,
		onnodeselect,
		onrestore,
		class: className = '',
	}: Props = $props();

	const summary = createResource(() => getAgentLoop(sessionId));
	const graph = createResource(() => getLoopGraph(sessionId));
	const iterations = createResource(() =>
		listLoopIterations(sessionId, { limit: 200 }),
	);
	const variables = createResource(() => listLoopVariables(sessionId));
	const checkpoints = createResource(() => listLoopCheckpoints(sessionId));
	const timeline = createResource(() => listLoopTimeline(sessionId));
	const messages = createResource(() => listLoopMessages(sessionId));

	const STATE: Record<TabDataKey, TabState> = {
		summary,
		graph,
		iterations,
		variables,
		checkpoints,
		timeline,
		messages,
	};

	const RELOAD: Record<TabDataKey, () => Promise<void>> = {
		summary: () => summary.reload(),
		graph: () => graph.reload(),
		iterations: () => iterations.reload(),
		variables: () => variables.reload(),
		checkpoints: () => checkpoints.reload(),
		timeline: () => timeline.reload(),
		messages: () => messages.reload(),
	};

	/** Sources already pulled for this `sessionId#revision`, so a tab loads once. */
	let seen = $state<Partial<Record<TabDataKey, string>>>({});

	$effect(() => {
		if (!sessionId) return;
		const token = `${sessionId}#${revision}`;
		for (const key of TAB_DATA[tab]) {
			if (seen[key] === token) continue;
			seen = { ...seen, [key]: token };
			void RELOAD[key]();
		}
	});

	const items = $derived(tabs.map((id) => ({ id, label: TAB_LABELS[id] })));
	const primary = $derived(TAB_DATA[tab][0]);
	const active = $derived(STATE[primary]);
	const loading = $derived(active.loading && active.data === null);
	const error = $derived(active.error);

	const transcriptMessages = $derived(messages.data?.items ?? []);
	/** The transcript owns its scroll viewport, so the pane yields scrolling to it. */
	const showTranscript = $derived(
		tab === 'messages' && !loading && !error && transcriptMessages.length > 0,
	);

	const loop = $derived(summary.data);
	const iterationList = $derived(iterations.data?.items ?? []);
	const toolCalls = $derived(iterationList.flatMap((row) => row.toolCalls));
	const failedCalls = $derived(
		toolCalls.filter((call) => call.status !== 'completed').length,
	);
	const toolFrequency = $derived.by(() => {
		const counts: Record<string, { count: number; failed: number }> = {};
		for (const call of toolCalls) {
			const entry = counts[call.name] ?? { count: 0, failed: 0 };
			entry.count += 1;
			if (call.status !== 'completed') entry.failed += 1;
			counts[call.name] = entry;
		}
		return Object.entries(counts)
			.map(([tool, entry]) => ({ tool, ...entry }))
			.sort((a, b) => b.count - a.count);
	});
	const peakCalls = $derived(toolFrequency[0]?.count ?? 1);

	const variableColumns: Column<LoopVariable>[] = [
		{ key: 'key', header: 'Key', text: (row) => row.key },
		{ key: 'value', header: 'Value', text: (row) => row.value },
	];

	// Decision graph as display nodes, so history and live progress share
	// one projection bridge instead of a static snapshot.
	const decisionNodes = $derived<DisplayNode[]>(
		(graph.data?.nodes ?? []).map((node) => ({
			id: node.id,
			label: node.label,
			kind: node.kind,
			status: node.status,
			iteration: node.iteration,
		})),
	);
	const decisionEdges = $derived<DisplayEdge[]>(
		(graph.data?.edges ?? []).map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			label: edge.label,
			kind: edge.kind,
			taken: edge.taken,
		})),
	);

	/** Shared bridge: history and live frames resolve names identically. */
	function matchDecisionNode(name: string): string | null {
		return matchDecisionNodeId(decisionNodes, name);
	}

	// Worst tool-call status per node from recorded iterations; live frames
	// override history while a run is active.
	const historyNodeStatuses = $derived.by(() => {
		const statuses: Record<string, string> = {};
		const rank = (status: string): number =>
			status === 'failed' ? 2 : status === 'running' ? 1 : 0;
		for (const iteration of iterationList) {
			for (const call of iteration.toolCalls) {
				const nodeId = matchDecisionNode(call.name);
				if (!nodeId) continue;
				const status = call.status === 'completed' ? 'completed' : 'failed';
				if (rank(status) >= rank(statuses[nodeId] ?? 'completed')) {
					statuses[nodeId] = status;
				}
			}
		}
		return statuses;
	});

	const liveNodeStatuses = $derived.by(() => {
		const statuses: Record<string, string> = {};
		for (const [name, status] of Object.entries(liveStatuses)) {
			const nodeId = matchDecisionNode(name);
			if (nodeId) statuses[nodeId] = status;
		}
		return statuses;
	});

	const decisionOverlayNodes = $derived.by(() => {
		const overlay = projectExecutionOverlay(decisionNodes, {
			currentNode: liveNode,
			liveStatuses: { ...historyNodeStatuses, ...liveNodeStatuses },
			executedNodes: iterationList.flatMap((iteration) =>
				iteration.toolCalls
					.filter((call) => call.status === 'completed')
					.map((call) => matchDecisionNode(call.name))
					.filter((id): id is string => id !== null),
			),
		});
		return applyExecutionOverlay(decisionNodes, overlay);
	});

	let selectedGraphNode = $state<string | null>(null);
	let graphExplorer = $state<{ focus: (id: string) => void } | null>(null);

	$effect(() => {
		if (focusId) graphExplorer?.focus(focusId);
	});

	function handleGraphSelect(id: string | null): void {
		selectedGraphNode = id;
		onnodeselect?.(id);
	}

	/** History tool cards focus the graph like live tool cards do. */
	function handleToolFocus(entry: { id: string; name: string }): void {
		const nodeId = matchDecisionNode(entry.name);
		if (!nodeId) return;
		selectedGraphNode = nodeId;
		onnodeselect?.(nodeId);
		graphExplorer?.focus(nodeId);
	}

	const selectedGraphIteration = $derived(
		decisionNodes.find((node) => node.id === selectedGraphNode)?.iteration ??
			null,
	);

	const visibleIterations = $derived(
		selectedGraphIteration === null
			? iterationList
			: iterationList.filter(
					(iteration) => iteration.index === selectedGraphIteration,
				),
	);

	const visibleTools = $derived(
		selectedGraphNode === null
			? toolCalls
			: toolCalls.filter(
					(call) => matchDecisionNode(call.name) === selectedGraphNode,
				),
	);
</script>

<div class={cn('flex min-h-0 flex-col', className)}>
	<Segmented
		{items}
		bind:value={tab}
		size="sm"
		class="shrink-0 px-1"
		panelId="session-panel"
	/>

	<div
		id="session-panel"
		role="tabpanel"
		aria-label="Session sections"
		class={cn(
			'min-h-0 flex-1',
			showTranscript ? 'flex flex-col' : 'overflow-y-auto px-3 py-3',
		)}
	>
		{#if !sessionId}
			<p class="text-caption text-muted-foreground">
				Select a session to inspect its run.
			</p>
		{:else if loading}
			<div class="space-y-2">
				<Skeleton shape="block" height="72px" class="rounded-lg" />
				<Skeleton shape="block" height="72px" class="rounded-lg" />
			</div>
		{:else if error}
			<ErrorState
				title="Failed to load {TAB_LABELS[tab]}"
				description={error}
				onretry={() => void active.reload()}
			/>
		{:else if tab === 'overview'}
			{#if loop}
				<div class="mb-3 flex items-center gap-2">
					<StatusBadge status={loop.status} size="sm" />
					<span class="font-mono text-micro text-muted-foreground"
						>{loop.id}</span
					>
				</div>
				<Card title="Run facts">
					<KeyValueList
						items={[
							{ key: 'profile', value: loop.profileId ?? '—' },
							{
								key: 'iterations',
								value: formatNumber(loop.iteration),
							},
							{ key: 'tool calls', value: formatNumber(loop.toolCalls) },
							{ key: 'duration', value: formatDuration(loop.durationMs) },
							{ key: 'started', value: formatDateTime(loop.startedAt) },
							{
								key: 'ended',
								value: loop.endedAt ? formatDateTime(loop.endedAt) : '—',
							},
						]}
					/>
				</Card>
			{/if}
		{:else if tab === 'graph'}
			<GraphExplorer
				bind:this={graphExplorer}
				nodes={decisionOverlayNodes}
				edges={decisionEdges}
				preset="decision"
				loading={graph.loading && graph.data === null}
				error={graph.error}
				onretry={() => void graph.reload()}
				selectedId={selectedGraphNode}
				onselect={handleGraphSelect}
			/>
		{:else if tab === 'iterations'}
			{#if selectedGraphNode !== null}
				<div class="mb-2 flex items-center gap-2">
					<p class="text-micro text-muted-foreground">
						Filtered by graph node “{selectedGraphNode}”
					</p>
					<button
						type="button"
						class="text-micro text-foreground underline-offset-2 hover:underline"
						onclick={() => handleGraphSelect(null)}
					>
						Clear
					</button>
				</div>
			{/if}
			{#if visibleIterations.length === 0}
				<p class="text-caption text-muted-foreground">
					{selectedGraphNode !== null
						? 'No iterations for the selected graph node.'
						: 'No iteration records for this session.'}
				</p>
			{:else}
				<ul class="space-y-2">
					{#each visibleIterations as iteration (iteration.index)}
						<li
							class="flex items-start justify-between gap-3 border-b border-border/60 pb-2 last:border-0 last:pb-0"
						>
							<p class="min-w-0 text-caption">
								<span class="font-mono text-muted-foreground"
									>#{iteration.index}</span
								>
								<span class="ml-2 text-foreground">
									{iteration.summary || 'No response text'}
								</span>
							</p>
							<div class="flex shrink-0 items-center gap-2">
								<span class="text-micro tabular-nums text-muted-foreground">
									{iteration.toolCalls.length} calls ·
									{formatDuration(iteration.durationMs)}
								</span>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		{:else if tab === 'variables'}
			<DataTable
				columns={variableColumns}
				rows={variables.data ?? []}
				rowKey={(row) => row.key}
				dense
				emptyTitle="No variables"
			/>
		{:else if tab === 'checkpoints'}
			{#if (checkpoints.data ?? []).length === 0}
				<p class="text-caption text-muted-foreground">
					No checkpoints recorded for this session.
				</p>
			{:else}
				<div class="space-y-2">
					{#each checkpoints.data ?? [] as checkpoint (checkpoint.id)}
						<Card
							title="{checkpoint.kind} · #{checkpoint.chainPosition ?? '—'}"
						>
							{#snippet actions()}
								<StatusBadge status={checkpoint.status} size="sm" dot={false} />
							{/snippet}
							<p class="text-caption text-muted-foreground">
								{formatBytes(checkpoint.sizeBytes)} ·
								{formatDateTime(checkpoint.createdAt)}
							</p>
							{#if (checkpoint.tags ?? []).length > 0}
								<div class="mt-1.5 flex flex-wrap gap-1.5">
									{#each checkpoint.tags ?? [] as tag (tag)}
										<Badge variant="outline">{tag}</Badge>
									{/each}
								</div>
							{/if}
							{#if onrestore}
								<Button
									variant="ghost"
									size="sm"
									class="mt-2"
									disabled={busy}
									onclick={() => onrestore(checkpoint)}
								>
									Restore
								</Button>
							{/if}
						</Card>
					{/each}
				</div>
			{/if}
		{:else if tab === 'tools'}
			{#if selectedGraphNode !== null}
				<div class="mb-2 flex items-center gap-2">
					<p class="text-micro text-muted-foreground">
						Filtered by graph node “{selectedGraphNode}”
					</p>
					<button
						type="button"
						class="text-micro text-foreground underline-offset-2 hover:underline"
						onclick={() => handleGraphSelect(null)}
					>
						Clear
					</button>
				</div>
			{/if}
			{#if visibleTools.length === 0}
				<p class="text-caption text-muted-foreground">
					{selectedGraphNode !== null
						? 'No tool calls for the selected graph node.'
						: 'No tool calls recorded for this session.'}
				</p>
			{:else}
				<div class="space-y-2">
					{#each visibleTools as entry (entry.id)}
						{@const toolNodeId = matchDecisionNode(entry.name)}
						<div
							role="button"
							tabindex={toolNodeId ? 0 : -1}
							aria-label={toolNodeId
								? `Locate tool ${entry.name} on decision graph`
								: `Tool ${entry.name}`}
							onclick={() => handleToolFocus(entry)}
							onkeydown={(event) => {
								if (event.key === 'Enter' || event.key === ' ') {
									event.preventDefault();
									handleToolFocus(entry);
								}
							}}
							class={cn(
								'rounded-lg',
								toolNodeId && 'cursor-pointer',
								toolNodeId &&
									selectedGraphNode === toolNodeId &&
									'ring-2 ring-warning',
							)}
						>
							<ToolCallCard {entry} />
						</div>
					{/each}
				</div>
			{/if}
		{:else if tab === 'timeline'}
			{#if (timeline.data ?? []).length === 0}
				<p class="text-caption text-muted-foreground">
					No timeline events for this session.
				</p>
			{:else}
				<div class="flex items-start gap-3">
					<Timeline entries={timeline.data ?? []} class="min-w-0 flex-1" />
					<TimelineOutline
						entries={timeline.data ?? []}
						class="hidden w-44 xl:block"
					/>
				</div>
			{/if}
		{:else if tab === 'analysis'}
			<div class="space-y-3">
				<Card title="Call totals">
					<KeyValueList
						items={[
							{
								key: 'iterations',
								value: formatNumber(iterationList.length),
							},
							{ key: 'tool calls', value: formatNumber(toolCalls.length) },
							{ key: 'failed', value: formatNumber(failedCalls) },
						]}
						dense
					/>
				</Card>
				{#if toolFrequency.length === 0}
					<p class="text-caption text-muted-foreground">
						Nothing to aggregate yet.
					</p>
				{:else}
					<Card title="Tool frequency">
						<ul class="space-y-2">
							{#each toolFrequency as item (item.tool)}
								<li class="flex items-center gap-3">
									<span class="w-28 shrink-0 truncate font-mono text-caption"
										>{item.tool}</span
									>
									<span
										class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted"
									>
										<span
											class="block h-full rounded-full bg-info"
											style:width="{(item.count / peakCalls) * 100}%"
										></span>
									</span>
									<span
										class="w-16 shrink-0 text-right text-caption tabular-nums text-muted-foreground"
									>
										{formatNumber(item.count)}
										{#if item.failed > 0}
											<span class="text-destructive">
												· {formatNumber(item.failed)}</span
											>
										{/if}
									</span>
								</li>
							{/each}
						</ul>
					</Card>
				{/if}
			</div>
		{:else if tab === 'messages'}
			{#if showTranscript}
				<TranscriptScroller
					items={transcriptMessages}
					itemKey={(message) => message.id}
					resetKey={sessionId}
					contentClass="mx-auto max-w-3xl px-3 py-3"
					class="min-h-0 flex-1"
				>
					{#snippet renderItem(message: LoopMessage)}
						<MessageBubble {message} />
					{/snippet}
				</TranscriptScroller>
			{:else}
				<p class="text-caption text-muted-foreground">
					No messages recorded for this session.
				</p>
			{/if}
		{/if}
	</div>
</div>
