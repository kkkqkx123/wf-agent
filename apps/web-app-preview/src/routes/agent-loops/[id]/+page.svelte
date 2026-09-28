<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import GraphExplorer, {
		type GraphOverlay,
	} from '$lib/components/domain/GraphExplorer.svelte';
	import {
		cancelAgentLoop,
		createAgentLoopCheckpoint,
		getAgentLoopAnalysis,
		getAgentLoopDetail,
		getAgentLoopMessages,
		getAgentLoopVariables,
		pauseAgentLoop,
		resumeAgentLoop,
		restoreAgentLoopCheckpoint,
		type AgentLoopAnalysis,
	} from '$lib/services/agent-loops';
	import { listLoopCheckpoints } from '$lib/services/checkpoints';
	import type {
		AgentLoopDetail,
		Checkpoint,
		LoopMessage,
		LoopVariable,
	} from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';

	const TABS = [
		{ id: 'messages', label: 'Messages' },
		{ id: 'variables', label: 'Variables' },
		{ id: 'graph', label: 'Graph' },
		{ id: 'analysis', label: 'Analysis' },
		{ id: 'checkpoints', label: 'Checkpoints' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'messages',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'messages' ? '' : tab });
	});

	let detail = $state<AgentLoopDetail | null>(null);
	let detailError = $state<string | null>(null);
	let loopMessages = $state<LoopMessage[]>([]);
	let messagesError = $state<string | null>(null);
	let loopVariables = $state<LoopVariable[]>([]);
	let variablesError = $state<string | null>(null);
	let loopCheckpoints = $state<Checkpoint[]>([]);
	let checkpointsError = $state<string | null>(null);
	let analysis = $state<AgentLoopAnalysis | null>(null);
	let analysisError = $state<string | null>(null);
	let analysisLoading = $state(false);

	let graphNodeId = $state<string | null>(null);
	let activeOverlay = $state<string | null>(null);
	let cancelArmed = $state(false);
	let controlBusy = $state(false);

	let seenDetail = $state('');
	let seenMessages = $state('');
	let seenVariables = $state('');
	let seenCheckpoints = $state('');
	let seenAnalysis = $state('');

	const loop = $derived(detail);

	const nodes = $derived<DisplayNode[]>(
		(detail?.graph.nodes ?? []).map((node) => ({
			id: node.id,
			label: node.label,
			kind: node.kind,
			status: node.status,
			iteration: node.iteration,
		})),
	);

	const edges = $derived<DisplayEdge[]>(
		(detail?.graph.edges ?? []).map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			label: edge.label,
		})),
	);

	const overlays = $derived.by<GraphOverlay[]>(() => {
		const errorIds = nodes
			.filter((node) => node.status === 'failed')
			.map((node) => node.id);
		return errorIds.length > 0
			? [{ id: 'errors', label: 'Error nodes', ids: errorIds }]
			: [];
	});

	const peakToolCount = $derived(
		Math.max(
			1,
			...((analysis?.toolFrequency ?? detail?.analysis.toolFrequency ?? []).map(
				(entry) => entry.count,
			)),
		),
	);

	const toolFrequency = $derived(
		analysis?.toolFrequency ?? detail?.analysis.toolFrequency ?? [],
	);

	async function loadDetail(id: string): Promise<void> {
		detailError = null;
		try {
			detail = await getAgentLoopDetail(id);
		} catch (e) {
			detailError = e instanceof Error ? e.message : 'Failed to load loop.';
			detail = null;
		}
	}

	async function loadTab(id: string, current: string): Promise<void> {
		try {
			if (current === 'messages' && seenMessages !== id) {
				seenMessages = id;
				messagesError = null;
				loopMessages = await getAgentLoopMessages(id);
			} else if (current === 'variables' && seenVariables !== id) {
				seenVariables = id;
				variablesError = null;
				loopVariables = await getAgentLoopVariables(id);
			} else if (current === 'checkpoints' && seenCheckpoints !== id) {
				seenCheckpoints = id;
				checkpointsError = null;
				loopCheckpoints = await listLoopCheckpoints(id);
			} else if (current === 'analysis' && seenAnalysis !== id) {
				seenAnalysis = id;
				analysisError = null;
				analysisLoading = true;
				try {
					analysis = await getAgentLoopAnalysis(id);
				} catch (e) {
					analysisError =
						e instanceof Error ? e.message : 'Analysis failed to load.';
					analysis = null;
				} finally {
					analysisLoading = false;
				}
			}
		} catch (e) {
			const message = e instanceof Error ? e.message : 'Segment failed.';
			if (current === 'messages') {
				seenMessages = '';
				messagesError = message;
			} else if (current === 'variables') {
				seenVariables = '';
				variablesError = message;
			} else if (current === 'checkpoints') {
				seenCheckpoints = '';
				checkpointsError = message;
			}
		}
	}

	async function runControl(
		label: string,
		action: (id: string) => Promise<void>,
	): Promise<void> {
		const id = page.params.id;
		if (!id) return;
		controlBusy = true;
		try {
			await action(id);
			toasts.success(`${label} done`);
			seenDetail = '';
			await loadDetail(id);
		} catch (e) {
			toasts.error(
				`${label} failed`,
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			controlBusy = false;
		}
	}

	async function runCheckpoint(): Promise<void> {
		const id = page.params.id;
		if (!id) return;
		controlBusy = true;
		try {
			await createAgentLoopCheckpoint(id);
			toasts.success('Checkpoint created');
			seenCheckpoints = '';
			if (tab === 'checkpoints') await loadTab(id, 'checkpoints');
		} catch (e) {
			toasts.error(
				'Checkpoint failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			controlBusy = false;
		}
	}

	async function runRestore(checkpointId: string): Promise<void> {
		const id = page.params.id;
		if (!id) return;
		try {
			await restoreAgentLoopCheckpoint(id, checkpointId);
			toasts.success('Checkpoint restored');
			seenDetail = '';
			await loadDetail(id);
		} catch (e) {
			toasts.error(
				'Restore failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	function resetFor(id: string): void {
		seenDetail = id;
		seenMessages = '';
		seenVariables = '';
		seenCheckpoints = '';
		seenAnalysis = '';
		detail = null;
		loopMessages = [];
		loopVariables = [];
		loopCheckpoints = [];
		analysis = null;
		cancelArmed = false;
		activeOverlay = null;
		graphNodeId = null;
	}

	onMount(() => {
		const id = page.params.id;
		if (!id) return;
		resetFor(id);
		void loadDetail(id);
		void loadTab(id, tab);
	});

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (seenDetail !== id) {
			resetFor(id);
			void loadDetail(id);
		}
		void loadTab(id, tab);
	});

	const variableColumns: Column<LoopVariable>[] = [
		{ key: 'key', header: 'Key', text: (row) => row.key },
		{ key: 'type', header: 'Type', text: (row) => row.type ?? '' },
		{ key: 'value', header: 'Value', text: (row) => row.value },
		{ key: 'scope', header: 'Scope', text: (row) => row.scope ?? '' },
		{
			key: 'updated',
			header: 'Updated',
			text: (row) => formatDateTime(row.updatedAt ?? null),
		},
	];
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={loop?.name ?? 'Agent loop'}
		description={detail?.summary ?? ''}
	>
		{#snippet meta()}
			{#if loop}
				<StatusBadge status={loop.status} />
				<Badge variant="outline"
					>iteration {loop.iteration}/{loop.maxIterations}</Badge
				>
				<span class="font-mono text-caption text-muted-foreground"
					>{loop.id}</span
				>
				<span class="text-caption text-muted-foreground">{loop.model}</span>
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pause"
				label="Pause loop"
				disabled={controlBusy}
				onclick={() => void runControl('Pause', pauseAgentLoop)}
			/>
			<IconButton
				icon="refresh"
				label="Resume loop"
				disabled={controlBusy}
				onclick={() => void runControl('Resume', resumeAgentLoop)}
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={controlBusy}
				onclick={() => void runCheckpoint()}
			>
				<Icon name="archive" size={13} />
				Checkpoint
			</Button>
			{#if cancelArmed}
				<Button
					variant="outline"
					size="sm"
					disabled={controlBusy}
					onclick={() => {
						cancelArmed = false;
						void runControl('Cancel', cancelAgentLoop);
					}}
				>
					Confirm cancel
				</Button>
				<Button
					variant="ghost"
					size="sm"
					onclick={() => (cancelArmed = false)}
				>
					Keep
				</Button>
			{:else}
				<Button
					variant="outline"
					size="sm"
					onclick={() => (cancelArmed = true)}
				>
					<Icon name="square" size={13} />
					Cancel
				</Button>
			{/if}
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" panelId="loop-panel" />

	<div
		id="loop-panel"
		role="tabpanel"
		aria-label="Agent loop sections"
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		{#if detailError && !detail}
			<ErrorState
				title="Agent loop failed to load"
				description={detailError}
				onretry={() => {
					const id = page.params.id;
					if (id) {
						resetFor(id);
						void loadDetail(id);
						void loadTab(id, tab);
					}
				}}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if tab === 'messages'}
			{#if messagesError}
				<ErrorState
					title="Messages failed to load"
					description={messagesError}
					onretry={() => {
						const id = page.params.id;
						if (id) {
							seenMessages = '';
							void loadTab(id, 'messages');
						}
					}}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="mx-auto flex max-w-3xl flex-col gap-3">
					{#each loopMessages as message (message.id)}
						<MessageBubble {message} />
					{/each}
				</div>
			{/if}

			<div
				class="mx-auto mt-4 flex max-w-3xl items-center justify-between gap-2 rounded-lg border border-border bg-card p-3"
			>
				<p class="text-caption text-muted-foreground">
					Follow-ups run in the live chat session for this loop.
				</p>
				<Button
					size="sm"
					onclick={() => {
						const id = page.params.id;
						if (id)
							void goto(resolve(`/chat?id=${encodeURIComponent(id)}`));
					}}
				>
					Continue in chat
					<Icon name="arrow-right" size={13} />
				</Button>
			</div>
		{:else if tab === 'variables'}
			{#if variablesError}
				<ErrorState
					title="Variables failed to load"
					description={variablesError}
					onretry={() => {
						const id = page.params.id;
						if (id) {
							seenVariables = '';
							void loadTab(id, 'variables');
						}
					}}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<Card title="Variables" bodyClass="p-0">
					<DataTable
						columns={variableColumns}
						rows={loopVariables}
						rowKey={(row) => row.key}
					/>
				</Card>
			{/if}
		{:else if tab === 'graph'}
			{#if !detail}
				<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
			{:else}
				<GraphExplorer
					{nodes}
					{edges}
					preset="decision"
					selectedId={graphNodeId}
					onselect={(id) => (graphNodeId = id)}
					overlays={overlays}
					{activeOverlay}
					onoverlay={(id) => (activeOverlay = id)}
				/>
				<Card title="Iterations" class="mt-3">
					<ul class="space-y-2">
						{#each detail.iterations as iteration (iteration.index)}
							<li
								class="flex items-start justify-between gap-3 border-b border-border/60 pb-2 last:border-0 last:pb-0"
							>
								<div class="min-w-0">
									<p class="text-caption">
										<span class="font-mono text-muted-foreground"
											>#{iteration.index}</span
										>
										<span class="ml-2">{iteration.summary}</span>
									</p>
								</div>
								<div class="flex shrink-0 items-center gap-2">
									<span class="text-micro tabular-nums text-muted-foreground">
										{formatDuration(iteration.durationMs)}
									</span>
									<StatusBadge status={iteration.status} size="sm" dot={false} />
								</div>
							</li>
						{/each}
					</ul>
				</Card>
			{/if}
		{:else if tab === 'analysis'}
			{#if analysisLoading}
				<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
			{:else if analysisError}
				<ErrorState
					title="Analysis failed to load"
					description={analysisError}
					onretry={() => {
						const id = page.params.id;
						if (id) {
							seenAnalysis = '';
							void loadTab(id, 'analysis');
						}
					}}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				{@const rootCause = analysis?.rootCause ?? detail?.analysis.rootCause ?? null}
				{@const errorChain = analysis?.errorChain ?? detail?.analysis.errorChain ?? []}
				{@const recoveryHints = analysis?.recoveryHints ?? detail?.analysis.recoveryHints ?? []}
				<div class="grid gap-3 lg:grid-cols-2">
					<Card title="Error analysis">
						<p class="text-caption">
							Root cause:
							<span class="text-foreground">
								{rootCause ?? 'None recorded'}
							</span>
						</p>
						{#if errorChain.length > 0}
							<ol class="mt-2 space-y-1">
								{#each errorChain as link, index (index)}
									<li class="text-caption text-destructive">{link}</li>
								{/each}
							</ol>
						{:else}
							<p class="mt-2 text-caption text-muted-foreground">
								No error chain for this loop.
							</p>
						{/if}
					</Card>
					<Card title="Recovery hints">
						{#if recoveryHints.length > 0}
							<ul class="space-y-1.5">
								{#each recoveryHints as hint, index (index)}
									<li class="flex items-start gap-1.5 text-caption">
										<Icon
											name="sparkles"
											size={12}
											class="mt-0.5 shrink-0 text-info"
										/>
										<span>{hint}</span>
									</li>
								{/each}
							</ul>
						{:else}
							<p class="text-caption text-muted-foreground">
								No recovery hints for this loop.
							</p>
						{/if}
					</Card>
					<Card title="Tool frequency" class="lg:col-span-2">
						{#if toolFrequency.length > 0}
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
												style:width={`${(item.count / peakToolCount) * 100}%`}
											></span>
										</span>
										<span
											class="w-8 shrink-0 text-right text-caption tabular-nums text-muted-foreground"
										>
											{formatNumber(item.count)}
										</span>
									</li>
								{/each}
							</ul>
						{:else}
							<p class="text-caption text-muted-foreground">
								No tool calls recorded for this loop.
							</p>
						{/if}
					</Card>
				</div>
			{/if}
		{:else}
			{#if checkpointsError}
				<ErrorState
					title="Checkpoints failed to load"
					description={checkpointsError}
					onretry={() => {
						const id = page.params.id;
						if (id) {
							seenCheckpoints = '';
							void loadTab(id, 'checkpoints');
						}
					}}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="space-y-2">
					{#each loopCheckpoints as checkpoint (checkpoint.id)}
						<Card title="{checkpoint.kind} · #{checkpoint.sequence}">
							{#snippet actions()}
								<Badge variant={checkpoint.restorable ? 'success' : 'neutral'}>
									{checkpoint.restorable ? 'restorable' : 'locked'}
								</Badge>
							{/snippet}
							<p class="text-caption text-muted-foreground">{checkpoint.note}</p>
							<p class="mt-1 text-micro text-muted-foreground">
								{checkpoint.actor} · {formatDateTime(checkpoint.createdAt)}
							</p>
							{#snippet footer()}
								<Button
									variant="ghost"
									size="sm"
									disabled={!checkpoint.restorable}
									title={checkpoint.restorable
										? 'Restore this checkpoint'
										: 'This checkpoint is locked and cannot be restored'}
									onclick={() => void runRestore(checkpoint.id)}
								>
									Restore
								</Button>
							{/snippet}
						</Card>
					{:else}
						<Card>
							<p class="text-caption text-muted-foreground">
								No checkpoints recorded for this loop.
							</p>
						</Card>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
</div>
