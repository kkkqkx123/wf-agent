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
	import type { Column } from '$lib/components/ui/table';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import MessageBubble from '$lib/components/domain/MessageBubble.svelte';
	import WorkflowGraph from '$lib/components/domain/WorkflowGraph.svelte';
	import { getAgentLoop } from '$lib/services/agent-loops';
	import { listCheckpointsByEntity } from '$lib/services/checkpoints';
	import type { AgentLoopDetail, Checkpoint, LoopMessage, LoopVariable } from '$lib/types/models';
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

	let tab = $state('messages');
	let draft = $state('');
	let detail = $state<AgentLoopDetail | null>(null);

	const safeDetail = $derived(detail ?? ({
		...({} as AgentLoopDetail),
		graph: { nodes: [], edges: [] },
		variables: [],
		messages: [],
		iterations: [],
		analysis: { rootCause: null, errorChain: [], recoveryHints: [], toolFrequency: [] },
	} as AgentLoopDetail));
	let checkpoints = $state<Checkpoint[]>([]);

	const loop = $derived(safeDetail);
	const loopVariables = $derived((detail?.variables ?? []) as LoopVariable[]);
	const loopMessages = $derived((detail?.messages ?? []) as LoopMessage[]);

	const variableColumns: Column<(typeof loopVariables)[number]>[] = [
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

	onMount(async () => {
		const id = page.params.id as string;
		const [detailRes, cpRes] = await Promise.allSettled([
			getAgentLoop(id),
			listCheckpointsByEntity(id),
		]);
		if (detailRes.status === 'fulfilled') detail = detailRes.value;
		if (cpRes.status === 'fulfilled') checkpoints = cpRes.value;
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={loop.name} description={safeDetail.summary}>
		{#snippet meta()}
			<StatusBadge status={loop.status} />
			<Badge variant="outline"
				>iteration {loop.iteration}/{loop.maxIterations}</Badge
			>
			<span class="font-mono text-caption text-muted-foreground">{loop.id}</span
			>
			<span class="text-caption text-muted-foreground">{loop.model}</span>
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pause"
				label="Pause loop"
				onclick={() => toasts.warning('Pause queued')}
			/>
			<IconButton
				icon="refresh"
				label="Resume loop"
				onclick={() => toasts.info('Resume queued')}
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => toasts.success('Checkpoint created')}
			>
				<Icon name="archive" size={13} />
				Checkpoint
			</Button>
			<Button
				variant="outline"
				size="sm"
				onclick={() => toasts.error('Cancel requires confirmation')}
			>
				<Icon name="square" size={13} />
				Cancel
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if tab === 'messages'}
			<div class="mx-auto flex max-w-3xl flex-col gap-3">
				{#each loopMessages as message (message.id)}
					<MessageBubble {message} />
				{/each}
			</div>

			<div
				class="mx-auto mt-4 max-w-3xl rounded-lg border border-border bg-card p-2"
			>
				<Textarea
					bind:value={draft}
					placeholder="Send a follow-up to this loop…"
					class="min-h-16 border-0"
				/>
				<div class="mt-2 flex items-center justify-between">
					<span class="text-micro text-muted-foreground"
						>Enter sends · Shift+Enter adds a line</span
					>
					<div class="flex items-center gap-2">
						<Button variant="ghost" size="sm" onclick={() => (draft = '')}
							>Clear</Button
						>
						<Button
							size="sm"
							disabled={draft.trim().length === 0}
							onclick={() => {
								toasts.success('Message queued');
								draft = '';
							}}
						>
							<Icon name="arrow-up" size={13} />
							Send
						</Button>
					</div>
				</div>
			</div>
		{:else if tab === 'variables'}
			<Card title="Variables" bodyClass="p-0">
				<DataTable
					columns={variableColumns}
					rows={loopVariables}
					rowKey={(row) => row.key}
				/>
			</Card>
		{:else if tab === 'graph'}
			<WorkflowGraph graph={safeDetail.graph} class="max-h-[26rem]" />
			<Card title="Iterations" class="mt-3">
				<ul class="space-y-2">
					{#each safeDetail.iterations as iteration (iteration.index)}
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
		{:else if tab === 'analysis'}
			<div class="grid gap-3 lg:grid-cols-2">
				<Card title="Error analysis">
					<p class="text-caption">
						Root cause:
						<span class="text-foreground">
							{safeDetail.analysis.rootCause ?? 'None recorded'}
						</span>
					</p>
					{#if safeDetail.analysis.errorChain.length > 0}
						<ol class="mt-2 space-y-1">
							{#each safeDetail.analysis.errorChain as link, index (index)}
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
						{#each safeDetail.analysis.recoveryHints as hint, index (index)}
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
						{#each safeDetail.analysis.toolFrequency as item (item.tool)}
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
												...safeDetail.analysis.toolFrequency.map(
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
			<div class="space-y-2">
				{#each checkpoints as checkpoint (checkpoint.id)}
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
								onclick={() => toasts.info('Restore queued')}
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
	</div>
</div>
