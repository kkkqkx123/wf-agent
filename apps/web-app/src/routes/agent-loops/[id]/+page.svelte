<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import type { Column } from '$lib/components/ui/table';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import WorkflowGraph from '$lib/components/domain/WorkflowGraph.svelte';
	import {
		getAgentLoop,
		pauseAgentLoop,
		resumeAgentLoop,
		cancelAgentLoop,
		createAgentLoopCheckpoint,
		restoreAgentLoopCheckpoint,
	} from '$lib/services/agent-loops';
	import { listCheckpointsByEntity } from '$lib/services/checkpoints';
	import type { Checkpoint, LoopVariable } from '$lib/types/models';
	import { createResource } from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '$lib/utils/format';

	const TABS = [
		{ id: 'messages', label: 'Messages' },
		{ id: 'variables', label: 'Variables' },
		{ id: 'graph', label: 'Graph' },
		{ id: 'analysis', label: 'Analysis' },
		{ id: 'checkpoints', label: 'Checkpoints' },
	];

	const id = $derived(page.params.id as string);

	let tab = $state('messages');

	const detail = createResource(() => getAgentLoop(id));
	// Agent-loop checkpoints are recorded per entity id; no paging endpoint exists.
	const checkpointList = createResource(() => listCheckpointsByEntity(id));
	const checkpoints = $derived(checkpointList.data ?? []);

	const TERMINAL = ['completed', 'failed', 'cancelled'];

	const loopStatus = $derived(detail.data?.status ?? '');
	let busy = $state(false);

	const variableColumns: Column<LoopVariable>[] = [
		{ key: 'key', header: 'Key', text: (row) => row.key },
		{ key: 'type', header: 'Type', text: (row) => row.type },
		{ key: 'value', header: 'Value', text: (row) => row.value },
		{ key: 'scope', header: 'Scope', text: (row) => row.scope },
		{
			key: 'updated',
			header: 'Updated',
			text: (row) => formatDateTime(row.updatedAt),
		},
	];

	function reloadAll(): void {
		void detail.reload();
		void checkpointList.reload();
	}

	onMount(reloadAll);

	async function step(action: 'pause' | 'resume'): Promise<void> {
		busy = true;
		try {
			if (action === 'pause') await pauseAgentLoop(id);
			else await resumeAgentLoop(id);
			await detail.reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Action failed');
		} finally {
			busy = false;
		}
	}

	async function recordCheckpoint(): Promise<void> {
		busy = true;
		try {
			await createAgentLoopCheckpoint(id);
			toasts.success('Checkpoint recorded');
			await Promise.all([checkpointList.reload(), detail.reload()]);
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Checkpoint failed');
		} finally {
			busy = false;
		}
	}

	// Both remaining mutations discard in-flight work, so they are confirm-gated.
	let target = $state<
		{ kind: 'cancel' } | { kind: 'restore'; checkpoint: Checkpoint } | null
	>(null);
	let confirming = $state(false);

	function ask(
		action: { kind: 'cancel' } | { kind: 'restore'; checkpoint: Checkpoint },
	): void {
		target = action;
		confirming = true;
	}

	const confirmCopy = $derived.by(() => {
		if (!target) return { title: '', detail: '' };
		return target.kind === 'cancel'
			? {
					title: 'Cancel agent loop',
					detail: `Loop ${id} stops at its next safe point and stays cancelled.`,
				}
			: {
					title: 'Restore checkpoint',
					detail: `Loop ${id} is rewound to checkpoint #${target.checkpoint.sequence}; later state is discarded.`,
				};
	});

	async function runConfirmed(): Promise<void> {
		const action = target;
		busy = true;
		try {
			if (action?.kind === 'cancel') {
				await cancelAgentLoop(id);
				toasts.success(`Cancelled loop ${id}`);
				await detail.reload();
			} else if (action?.kind === 'restore') {
				await restoreAgentLoopCheckpoint(id, action.checkpoint.id);
				toasts.success(`Restored checkpoint #${action.checkpoint.sequence}`);
				await Promise.all([detail.reload(), checkpointList.reload()]);
			}
			confirming = false;
			target = null;
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Action failed');
		} finally {
			busy = false;
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={detail.data?.name ?? 'Agent loop detail'}
		description={detail.data?.summary ?? ''}
	>
		{#snippet meta()}
			{#if detail.data}
				<StatusBadge status={detail.data.status} />
				<Badge variant="outline"
					>iteration {detail.data.iteration}/{detail.data.maxIterations}</Badge
				>
				<span class="font-mono text-caption text-muted-foreground"
					>{detail.data.id}</span
				>
				<span class="text-caption text-muted-foreground"
					>{detail.data.model}</span
				>
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton icon="refresh" label="Refresh loop" onclick={reloadAll} />
			<IconButton
				icon="pause"
				label="Pause loop"
				disabled={busy || loopStatus !== 'running'}
				onclick={() => void step('pause')}
			/>
			<IconButton
				icon="play"
				label="Resume loop"
				disabled={busy || !['paused', 'queued'].includes(loopStatus)}
				onclick={() => void step('resume')}
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={busy}
				onclick={recordCheckpoint}
			>
				<Icon name="archive" size={13} />
				Checkpoint
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={busy || TERMINAL.includes(loopStatus)}
				onclick={() => ask({ kind: 'cancel' })}
			>
				<Icon name="square" size={13} />
				Cancel
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if detail.loading && !detail.data}
			<div class="space-y-3">
				<Skeleton shape="block" height="72px" class="rounded-lg" />
				<Skeleton shape="block" height="72px" class="rounded-lg" />
				<Skeleton shape="block" height="72px" class="rounded-lg" />
			</div>
		{:else if detail.error}
			<EmptyState
				icon="alert-triangle"
				title="Failed to load agent loop"
				description={detail.error}
				class="rounded-lg border border-border bg-card"
			>
				{#snippet actions()}
					<Button variant="link" size="sm" onclick={() => detail.reload()}
						>Retry</Button
					>
				{/snippet}
			</EmptyState>
		{:else if detail.data}
			{@const loop = detail.data}
			{#if tab === 'messages'}
				<div class="mx-auto flex max-w-3xl flex-col gap-3">
					{#each loop.messages as message (message.id)}
						<MessageBubble {message} />
					{/each}
					{#if loop.messages.length === 0}
						<p class="text-caption text-muted-foreground">
							No messages recorded for this loop yet.
						</p>
					{/if}
				</div>
			{:else if tab === 'variables'}
				<Card title="Variables" bodyClass="p-0">
					<DataTable
						columns={variableColumns}
						rows={loop.variables}
						rowKey={(row) => row.key}
					/>
				</Card>
			{:else if tab === 'graph'}
				<WorkflowGraph graph={loop.graph} class="max-h-[26rem]" />
				<Card title="Iterations" class="mt-3">
					<ul class="space-y-2">
						{#each loop.iterations as iteration (iteration.index)}
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
									<StatusBadge
										status={iteration.status}
										size="sm"
										dot={false}
									/>
								</div>
							</li>
						{/each}
					</ul>
				</Card>
			{:else if tab === 'analysis'}
				<div class="grid gap-3 lg:grid-cols-2">
					<Card title="Error analysis">
						<p class="text-caption">
							Root cause:
							<span class="text-foreground">
								{loop.analysis.rootCause ?? 'None recorded'}
							</span>
						</p>
						{#if loop.analysis.errorChain.length > 0}
							<ol class="mt-2 space-y-1">
								{#each loop.analysis.errorChain as link, index (index)}
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
						<ul class="space-y-1.5">
							{#each loop.analysis.recoveryHints as hint, index (index)}
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
					</Card>
					<Card title="Tool frequency" class="lg:col-span-2">
						<ul class="space-y-2">
							{#each loop.analysis.toolFrequency as item (item.tool)}
								<li class="flex items-center gap-3">
									<span class="w-28 shrink-0 truncate font-mono text-caption"
										>{item.tool}</span
									>
									<span
										class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted"
									>
										<span
											class="block h-full rounded-full bg-info"
											style:width="{(item.count /
												Math.max(
													...loop.analysis.toolFrequency.map(
														(entry) => entry.count,
													),
												)) *
												100}%"
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
					</Card>
				</div>
			{:else}
				{#if checkpointList.loading && !checkpointList.data}
					<div class="space-y-2">
						{#each Array.from({ length: 3 }, (_, position) => position) as index (index)}
							<Skeleton shape="block" height="96px" class="rounded-lg" />
						{/each}
					</div>
				{:else if checkpointList.error}
					<EmptyState
						icon="alert-triangle"
						title="Failed to load checkpoints"
						description={checkpointList.error}
						class="rounded-lg border border-border bg-card"
					>
						{#snippet actions()}
							<Button
								variant="link"
								size="sm"
								onclick={() => checkpointList.reload()}>Retry</Button
							>
						{/snippet}
					</EmptyState>
				{:else if checkpoints.length === 0}
					<Card>
						<p class="text-caption text-muted-foreground">
							No checkpoints recorded for this loop.
						</p>
					</Card>
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
								<p class="mt-1 text-micro text-muted-foreground">
									{checkpoint.actor} · {formatDateTime(checkpoint.createdAt)}
								</p>
								{#snippet footer()}
									<Button
										variant="ghost"
										size="sm"
										disabled={busy || !checkpoint.restorable}
										onclick={() => ask({ kind: 'restore', checkpoint })}
									>
										Restore
									</Button>
								{/snippet}
							</Card>
						{/each}
					</div>
				{/if}
			{/if}
		{/if}
	</div>
</div>

<Dialog
	bind:open={confirming}
	title={confirmCopy.title}
	description={confirmCopy.detail}
	onclose={() => (target = null)}
>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (confirming = false)}
			>Keep running</Button
		>
		<Button
			variant="destructive"
			size="sm"
			disabled={!target || busy}
			onclick={() => void runConfirmed()}
		>
			{target?.kind === 'restore' ? 'Restore' : 'Cancel loop'}
		</Button>
	{/snippet}
</Dialog>
