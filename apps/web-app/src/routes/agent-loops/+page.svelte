<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import SessionInspector from '$lib/components/domain/SessionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import FilterBar from '$lib/components/domain/FilterBar.svelte';
	import LoadMorePager from '$lib/components/domain/LoadMorePager.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import { listAgentLoops } from '$lib/services/agent-loops';
	import type { AgentLoop } from '$lib/types/models';
	import { createCollection } from '$lib/stores/collection.svelte';
	import { sessions } from '$lib/stores/sessions.svelte';
	import { isSessionTab, type SessionTab } from '$lib/config/session-tabs';
	import { formatDuration, formatNumber } from '$lib/utils/format';
	import { appPath, gotoWithParams, parseListParams } from '$lib/utils/route';

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
	let tab = $state<SessionTab>(isSessionTab(initial.tab) ?? 'overview');

	const list = createCollection((params) => listAgentLoops(params));

	// Loop rows carry no name, so the filter searches the local session label.
	const filtered = $derived(
		list.items.filter((loop) => {
			const matchesStatus = !status || loop.status === status;
			const needle = query.trim().toLowerCase();
			return (
				matchesStatus &&
				(!needle ||
					sessions.label(loop.id).toLowerCase().includes(needle) ||
					loop.id.toLowerCase().includes(needle))
			);
		}),
	);

	$effect(() => {
		gotoWithParams(page.url, {
			q: query,
			status,
			id: selectedId ?? '',
			tab: tab === 'overview' ? '' : tab,
			page: String(Math.max(1, Math.ceil(list.loaded / list.pageSize))),
		});
	});

	onMount(() => {
		void list.loadPages(Number(initial.page) || 1);
	});
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
					onclick={() => void list.reload()}
				/>
			{/snippet}
		</PageHeader>

		<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			<FilterBar
				bind:query
				bind:status
				statusOptions={STATUS_OPTIONS}
				placeholder="Filter by session title or id…"
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
				<ErrorState
					title="Failed to load agent loops"
					description={list.error}
					onretry={() => list.reload()}
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				{#snippet loopName(loop: AgentLoop)}
					<span class="flex min-w-0 items-center gap-1.5">
						{#if sessions.isStarred(loop.id)}
							<Icon name="star" size={12} class="shrink-0 text-warning" />
						{/if}
						<span class="truncate">{sessions.label(loop.id)}</span>
					</span>
				{/snippet}
				{#snippet loopStatus(loop: AgentLoop)}
					<StatusBadge status={loop.status} size="sm" />
				{/snippet}
				{#snippet loopIterations(loop: AgentLoop)}
					<span class="text-caption tabular-nums text-muted-foreground">
						{formatNumber(loop.iteration)} · {formatNumber(loop.toolCalls)} calls
					</span>
				{/snippet}
				{#snippet loopProfile(loop: AgentLoop)}
					<span class="font-mono text-caption">
						{loop.profileId ?? '—'}
					</span>
				{/snippet}
				{#snippet loopDuration(loop: AgentLoop)}
					<span class="text-caption tabular-nums text-muted-foreground">
						{formatDuration(loop.durationMs)}
					</span>
				{/snippet}
				{#snippet loopLinks(loop: AgentLoop)}
					<span class="flex items-center justify-end gap-1">
						<Button
							variant="link"
							size="sm"
							href={appPath(`/chat?id=${loop.id}`)}
							class="px-1">Chat</Button
						>
						<Button
							variant="link"
							size="sm"
							href={appPath(`/agent-loops/${loop.id}`)}
							class="px-1">Detail</Button
						>
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
							{ key: 'name', header: 'Session', cell: loopName },
							{ key: 'status', header: 'Status', cell: loopStatus },
							{
								key: 'iteration',
								header: 'Progress',
								cell: loopIterations,
							},
							{ key: 'profile', header: 'Profile', cell: loopProfile },
							{
								key: 'duration',
								header: 'Duration',
								align: 'right',
								cell: loopDuration,
							},
							{
								key: 'links',
								header: 'Open',
								align: 'right',
								cell: loopLinks,
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
		<SessionInspector sessionId={selectedId ?? ''} bind:tab class="h-full" />
	{/snippet}
</SplitView>
