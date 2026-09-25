<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import FilterBar from '$lib/components/domain/FilterBar.svelte';
	import LoadMorePager from '$lib/components/domain/LoadMorePager.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Progress from '$lib/components/ui/Progress.svelte';
	import { listAgentLoops, getAgentLoop } from '$lib/services/agent-loops';
	import type { AgentLoop, AgentLoopDetail } from '$lib/types/models';
	import {
		createCollection,
		createResource,
	} from '$lib/stores/collection.svelte';
	import { formatNumber, formatRelativeTime } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const STATUS_OPTIONS = [
		{ value: 'running', label: 'Running' },
		{ value: 'paused', label: 'Paused' },
		{ value: 'completed', label: 'Completed' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'queued', label: 'Queued' },
		{ value: 'cancelled', label: 'Cancelled' },
	];

	const initial = parseListParams(page.url);

	let query = $state(initial.q ?? '');
	let status = $state(initial.status ?? '');
	let selectedId = $state<string | null>(initial.id ?? null);

	const list = createCollection((params) => listAgentLoops(params));
	const detail = createResource<AgentLoopDetail | null>(async () => {
		if (!selectedId) return null;
		return getAgentLoop(selectedId);
	});

	const filtered = $derived(
		list.items.filter((loop) => {
			const matchesStatus = !status || loop.status === status;
			const needle = query.trim().toLowerCase();
			return (
				matchesStatus && (!needle || loop.name.toLowerCase().includes(needle))
			);
		}),
	);

	$effect(() => {
		if (selectedId) void detail.reload();
	});

	$effect(() => {
		if (!selectedId && list.loaded > 0 && !list.loading) {
			selectedId = list.items[0].id;
		}
	});

	$effect(() => {
		gotoWithParams(page.url, {
			q: query,
			status,
			id: selectedId ?? '',
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => void list.loadPages(Number(initial.page) || 1));
</script>

<SplitView
	inspectorTitle="Loop detail"
	inspectorOpen={selectedId !== null}
	oninspectorclose={() => (selectedId = null)}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col">
		<PageHeader
			title="Agent loops"
			description="Autonomous runs with iterations, messages, variables and checkpoints."
		>
			{#snippet actions()}
				<IconButton
					icon="refresh"
					label="Refresh"
					onclick={() => {
						list.reload();
						if (selectedId) detail.reload();
					}}
				/>
			{/snippet}
		</PageHeader>

		<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			<FilterBar
				bind:query
				bind:status
				statusOptions={STATUS_OPTIONS}
				placeholder="Filter by name or tag…"
				class="mb-3"
			>
				{#snippet trailing()}
					<span class="text-caption text-muted-foreground"
						>{filtered.length} shown</span
					>
				{/snippet}
			</FilterBar>

			{#if list.loading && list.loaded === 0}
				<Card bodyClass="p-0">
					<div class="space-y-2 p-3">
						{#each Array.from({ length: 5 }, (_, position) => position) as index (index)}
							<Skeleton shape="block" height="38px" class="rounded-md" />
						{/each}
					</div>
				</Card>
			{:else if list.error}
				<EmptyState
					icon="alert-triangle"
					title="Failed to load agent loops"
					description={list.error}
					class="rounded-lg border border-border bg-card"
				>
					{#snippet actions()}
						<Button variant="link" size="sm" onclick={() => list.reload()}
							>Retry</Button
						>
					{/snippet}
				</EmptyState>
			{:else}
				{#snippet loopName(loop: AgentLoop)}
					<span class="flex items-center gap-1.5">
						{#if loop.starred}
							<Icon name="star" size={12} class="shrink-0 text-warning" />
						{/if}
						<span class="truncate">{loop.name}</span>
					</span>
				{/snippet}
				{#snippet loopStatus(loop: AgentLoop)}
					<StatusBadge status={loop.status} size="sm" />
				{/snippet}
				{#snippet loopIterations(loop: AgentLoop)}
					<span class="text-caption tabular-nums text-muted-foreground">
						{loop.iteration}/{loop.maxIterations}
					</span>
				{/snippet}
				{#snippet loopModel(loop: AgentLoop)}
					<span class="font-mono text-caption">{loop.model}</span>
				{/snippet}
				{#snippet loopTokens(loop: AgentLoop)}
					<span class="text-caption tabular-nums text-muted-foreground">
						{formatNumber(loop.tokens)}
					</span>
				{/snippet}
				{#snippet loopUpdated(loop: AgentLoop)}
					<span class="text-caption text-muted-foreground">
						{formatRelativeTime(loop.updatedAt)}
					</span>
				{/snippet}
				<Card bodyClass="p-0">
					<DataTable
						rows={filtered}
						rowKey={(row) => row.id}
						selectedKey={selectedId}
						onrowclick={(row) => (selectedId = row.id)}
						emptyTitle="No loops match"
						columns={[
							{ key: 'name', header: 'Loop', cell: loopName },
							{ key: 'status', header: 'Status', cell: loopStatus },
							{
								key: 'iteration',
								header: 'Iteration',
								cell: loopIterations,
							},
							{ key: 'model', header: 'Model', cell: loopModel },
							{
								key: 'tokens',
								header: 'Tokens',
								align: 'right',
								cell: loopTokens,
							},
							{
								key: 'updated',
								header: 'Updated',
								align: 'right',
								cell: loopUpdated,
							},
						]}
					/>
				</Card>
			{/if}

			<LoadMorePager
				shown={list.loaded}
				hasMore={list.hasMore}
				loading={list.loading}
				pageSize={list.pageSize}
				onloadmore={() => list.loadMore()}
				class="mt-3 rounded-lg border border-border bg-card"
			/>
		</div>
	</div>

	{#snippet inspector()}
		{#if detail.loading && !detail.data}
			<div class="space-y-3 p-4">
				<Skeleton lines={2} />
				<Skeleton shape="block" height="120px" class="rounded-lg" />
				<Skeleton lines={4} />
			</div>
		{:else if detail.error}
			<EmptyState
				icon="alert-triangle"
				title="Failed to load detail"
				description={detail.error}
				class="m-4"
			>
				{#snippet actions()}
					<Button variant="link" size="sm" onclick={() => detail.reload()}
						>Retry</Button
					>
				{/snippet}
			</EmptyState>
		{:else if detail.data}
			{@const selected = detail.data}
			<div class="flex h-full min-h-0 flex-col">
				<div class="border-b border-border px-3 py-3">
					<div class="flex items-start justify-between gap-2">
						<div class="min-w-0">
							<h2 class="truncate text-title font-semibold">{selected.name}</h2>
							<p class="mt-0.5 font-mono text-micro text-muted-foreground">
								{selected.id}
							</p>
						</div>
						<StatusBadge status={selected.status} />
					</div>
					<p class="mt-2 text-caption text-muted-foreground">
						{selected.summary}
					</p>
					<div class="mt-3">
						<div
							class="flex items-center justify-between text-micro text-muted-foreground"
						>
							<span>Iterations</span>
							<span class="tabular-nums"
								>{selected.iteration}/{selected.maxIterations}</span
							>
						</div>
						<Progress
							value={selected.iteration / Math.max(1, selected.maxIterations)}
							tone="running"
							class="mt-1"
						/>
					</div>
				</div>

				<div class="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
					<Card title="Run facts">
						<KeyValueList
							items={[
								{ key: 'model', value: selected.model },
								{ key: 'tokens', value: formatNumber(selected.tokens) },
								{
									key: 'checkpoints',
									value: formatNumber(selected.checkpoints),
								},
								{ key: 'errors', value: formatNumber(selected.errors) },
								{
									key: 'started',
									value: formatRelativeTime(selected.startedAt),
								},
								{
									key: 'updated',
									value: formatRelativeTime(selected.updatedAt),
								},
							]}
							dense
						/>
					</Card>

					<Card title="Tags">
						<div class="flex flex-wrap gap-1.5">
							{#each selected.tags as tag (tag)}
								<Badge variant="outline" size="sm">{tag}</Badge>
							{:else}
								<span class="text-caption text-muted-foreground">No tags</span>
							{/each}
						</div>
					</Card>

					<Button
						variant="outline"
						size="sm"
						href="/agent-loops/{selected.id}"
						class="w-full"
					>
						<Icon name="arrow-right" size={13} />
						Open full detail
					</Button>
				</div>
			</div>
		{/if}
	{/snippet}
</SplitView>
